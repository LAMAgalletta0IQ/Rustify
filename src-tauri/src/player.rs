use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use librespot::connect::{ConnectConfig, Spirc};
use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::{DeviceType, SessionConfig};
use librespot::core::session::Session;
use librespot::core::SpotifyUri;
use librespot::playback::config::{AudioFormat, PlayerConfig};
use librespot::playback::mixer::{self, MixerConfig};
use librespot::playback::player::{Player, PlayerEvent};
use librespot::playback::audio_backend;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::state::{events, AppState, PlaybackState, TokenStore, TrackInfo};
use crate::webapi::WebApi;

/// librespot's internal volume scale.
pub const MAX_VOLUME: u16 = u16::MAX;

pub fn percent_to_volume(percent: u8) -> u16 {
    ((percent.min(100) as u32 * MAX_VOLUME as u32) / 100) as u16
}

/// Builds the librespot session, player, mixer and Spirc, then spawns the
/// Spirc event loop and the PlayerEvent pump.
///
/// `Spirc` is what makes this app a real Connect receiver: it registers the
/// device with Spotify and handles remote commands from phones/other clients.
pub async fn start_session(
    app: AppHandle,
    credentials: Credentials,
    tokens: TokenStore,
    device_name: String,
    cache_dir: PathBuf,
) -> AppResult<StartedSession> {
    let session_config = SessionConfig::default();
    let player_config = PlayerConfig::default();
    let audio_format = AudioFormat::default();
    let mixer_config = MixerConfig::default();

    let connect_config = ConnectConfig {
        name: device_name,
        device_type: DeviceType::Computer,
        // Raw 0..=u16::MAX scale, NOT a percentage — librespot's own default
        // is u16::MAX / 2. Passing 50 here yields ~0.08% volume, i.e. silence.
        initial_volume: percent_to_volume(50),
        ..Default::default()
    };

    // Audio file cache keeps re-listens off the network; credentials cache lets
    // librespot reconnect without another OAuth round-trip.
    let cache = Cache::new(
        Some(cache_dir.as_path()),
        Some(cache_dir.as_path()),
        Some(cache_dir.join("files").as_path()),
        // Cap the audio cache so it cannot grow without bound on a small disk.
        Some(2 * 1024 * 1024 * 1024),
    )?;

    let sink_builder = audio_backend::find(None)
        .ok_or_else(|| AppError::Playback("no audio backend available".into()))?;
    let mixer_builder = mixer::find(None)
        .ok_or_else(|| AppError::Playback("no mixer available".into()))?;

    let session = Session::new(session_config, Some(cache));
    let mixer = mixer_builder(mixer_config)?;

    let player = Player::new(
        player_config,
        session.clone(),
        mixer.get_soft_volume(),
        move || sink_builder(None, audio_format),
    );

    // Must be taken before `player` is handed to Spirc.
    let event_rx = player.get_player_event_channel();

    let (spirc, spirc_task) = Spirc::new(
        connect_config,
        session.clone(),
        credentials,
        player,
        mixer.clone(),
    )
    .await?;

    // Spirc's event loop owns the Connect protocol; it must run for the
    // lifetime of the login.
    tauri::async_runtime::spawn(async move {
        spirc_task.await;
        log::info!("spirc task ended");
    });

    spawn_event_pump(app.clone(), event_rx, tokens);

    spirc.activate()?;

    // Seed the volume from the mixer rather than assuming a default, so the
    // slider is correct before the first VolumeChanged event arrives.
    {
        let state = app.state::<AppState>();
        state.playback.write().await.volume = mixer.volume();
    }

    Ok(StartedSession { session, spirc })
}

/// What `start_session` hands back; the caller pairs it with the refresh task
/// to form a [`SpotifySession`].
pub struct StartedSession {
    pub session: Session,
    pub spirc: Spirc,
}

/// Translates librespot `PlayerEvent`s into a single `PlaybackState` snapshot
/// pushed to the webview.
fn spawn_event_pump(
    app: AppHandle,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>,
    tokens: TokenStore,
) {
    tauri::async_runtime::spawn(async move {
        let api = WebApi::new();
        let cache: Arc<Mutex<HashMap<String, TrackInfo>>> = Arc::new(Mutex::new(HashMap::new()));

        while let Some(event) = rx.recv().await {
            let state = app.state::<AppState>();
            let mut pb = state.playback.write().await;

            let mut track_to_resolve: Option<SpotifyUri> = None;

            match event {
                PlayerEvent::Playing {
                    track_id,
                    position_ms,
                    ..
                } => {
                    pb.is_playing = true;
                    pb.is_loading = false;
                    pb.is_active_device = true;
                    pb.position_ms = position_ms;
                    track_to_resolve = Some(track_id);
                }
                PlayerEvent::Paused {
                    track_id,
                    position_ms,
                    ..
                } => {
                    pb.is_playing = false;
                    pb.is_loading = false;
                    pb.is_active_device = true;
                    pb.position_ms = position_ms;
                    track_to_resolve = Some(track_id);
                }
                PlayerEvent::Loading {
                    track_id,
                    position_ms,
                    ..
                } => {
                    pb.is_loading = true;
                    pb.is_active_device = true;
                    pb.position_ms = position_ms;
                    track_to_resolve = Some(track_id);
                }
                PlayerEvent::Stopped { .. } => {
                    pb.is_playing = false;
                    pb.is_loading = false;
                    pb.position_ms = 0;
                }
                PlayerEvent::PositionCorrection { position_ms, .. }
                | PlayerEvent::PositionChanged { position_ms, .. }
                | PlayerEvent::Seeked { position_ms, .. } => {
                    pb.position_ms = position_ms;
                }
                PlayerEvent::VolumeChanged { volume } => {
                    pb.volume = volume;
                }
                PlayerEvent::ShuffleChanged { shuffle } => {
                    pb.shuffle = shuffle;
                }
                PlayerEvent::RepeatChanged { context, track } => {
                    pb.repeat_context = context;
                    pb.repeat_track = track;
                }
                PlayerEvent::Unavailable { .. } => {
                    pb.is_loading = false;
                }
                PlayerEvent::SessionDisconnected { .. } => {
                    pb.is_active_device = false;
                    pb.is_playing = false;
                }
                // Preload/EndOfTrack/PlayRequestIdChanged and the remaining
                // variants carry no state the UI renders.
                _ => {
                    drop(pb);
                    continue;
                }
            }

            // Resolve metadata off the Web API rather than librespot's
            // AudioItem: it yields ready-to-use CDN cover URLs and a stable
            // shape. Cached per URI so repeats cost nothing.
            if let Some(uri) = track_to_resolve {
                let uri_str = uri.to_uri().unwrap_or_default();
                let changed = pb
                    .track
                    .as_ref()
                    .map(|t| t.uri != uri_str)
                    .unwrap_or(true);

                if changed && !uri_str.is_empty() {
                    let cached = cache.lock().await.get(&uri_str).cloned();
                    let info = match cached {
                        Some(i) => Some(i),
                        None => match fetch_track_info(&api, &tokens.get().await, &uri).await {
                            Ok(i) => {
                                cache.lock().await.insert(uri_str.clone(), i.clone());
                                Some(i)
                            }
                            Err(e) => {
                                log::warn!("metadata lookup failed for {uri_str}: {e}");
                                None
                            }
                        },
                    };
                    if let Some(i) = info {
                        pb.duration_ms = i.duration_ms;
                        pb.track = Some(i);
                    }
                }
            }

            let snapshot = pb.clone();
            drop(pb);

            if let Err(e) = app.emit(events::PLAYBACK, &snapshot) {
                log::warn!("failed to emit playback state: {e}");
            }
        }

        log::info!("player event pump ended");
    });
}

#[derive(Debug, Deserialize)]
struct ApiTrack {
    name: String,
    duration_ms: u32,
    artists: Vec<ApiNamed>,
    album: ApiAlbum,
}

#[derive(Debug, Deserialize)]
struct ApiNamed {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ApiAlbum {
    name: String,
    images: Vec<ApiImage>,
}

#[derive(Debug, Deserialize)]
struct ApiImage {
    url: String,
    width: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ApiEpisode {
    name: String,
    duration_ms: u32,
    images: Vec<ApiImage>,
    show: Option<ApiNamed>,
}

async fn fetch_track_info(api: &WebApi, token: &str, uri: &SpotifyUri) -> AppResult<TrackInfo> {
    let id = uri
        .to_id()
        .map_err(|e| AppError::Other(format!("bad spotify id: {e}")))?;
    let uri_str = uri
        .to_uri()
        .map_err(|e| AppError::Other(format!("bad spotify uri: {e}")))?;

    match uri.item_type() {
        "episode" => {
            let ep: ApiEpisode = api.get(token, &format!("/episodes/{id}"), &[]).await?;
            Ok(TrackInfo {
                uri: uri_str,
                name: ep.name,
                artists: ep.show.map(|s| vec![s.name]).unwrap_or_default(),
                album: String::new(),
                cover_url: pick_cover(&ep.images),
                duration_ms: ep.duration_ms,
            })
        }
        _ => {
            let tr: ApiTrack = api.get(token, &format!("/tracks/{id}"), &[]).await?;
            Ok(TrackInfo {
                uri: uri_str,
                name: tr.name,
                artists: tr.artists.into_iter().map(|a| a.name).collect(),
                album: tr.album.name,
                cover_url: pick_cover(&tr.album.images),
                duration_ms: tr.duration_ms,
            })
        }
    }
}

/// Spotify returns images largest-first. Prefer a mid-size one (~300px) so the
/// now-playing bar is not decoding a 640px JPEG on a low-end CPU.
fn pick_cover(images: &[ApiImage]) -> Option<String> {
    images
        .iter()
        .min_by_key(|i| (i.width.unwrap_or(640) as i32 - 300).abs())
        .map(|i| i.url.clone())
}

/// Rebuilds a fresh playback snapshot (used on reconnect / initial load).
pub async fn snapshot(state: &AppState) -> PlaybackState {
    state.playback.read().await.clone()
}
