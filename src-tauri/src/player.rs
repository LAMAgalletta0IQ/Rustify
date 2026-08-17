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

use crate::connect;
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

    // Deliberately NOT activated here. `Spirc::activate` makes this the active
    // Connect device, which pauses whatever is playing on the user's phone or
    // desktop client. Merely launching the app must not do that — it should
    // appear in the device list and stay idle until asked to play. Activation
    // happens on an explicit load (see `load_context`/`load_tracks`) or via
    // `activate_this_device`.

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
                    pb.set_position(position_ms);
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
                    pb.set_position(position_ms);
                    track_to_resolve = Some(track_id);
                }
                PlayerEvent::Loading {
                    track_id,
                    position_ms,
                    ..
                } => {
                    pb.is_loading = true;
                    pb.is_active_device = true;
                    pb.set_position(position_ms);
                    track_to_resolve = Some(track_id);
                }
                PlayerEvent::Stopped { .. } => {
                    pb.is_playing = false;
                    pb.is_loading = false;
                    pb.set_position(0);
                }
                PlayerEvent::PositionCorrection { position_ms, .. }
                | PlayerEvent::PositionChanged { position_ms, .. }
                | PlayerEvent::Seeked { position_ms, .. } => {
                    pb.set_position(position_ms);
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

            // Events like VolumeChanged carry no position, so bring it up to
            // date from the anchor rather than shipping a stale one.
            pb.refresh_position();

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

/// How often to ask Spotify what is playing elsewhere.
///
/// Only runs while this app is *not* the active device, so it costs nothing
/// during local playback. Slow enough to stay clear of rate limits, fast
/// enough that the UI is not visibly stale.
const REMOTE_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// Mirrors playback happening on other Connect devices into `PlaybackState`.
///
/// Without this the UI shows "Nothing playing" whenever the user is listening
/// on their phone, because librespot only reports audio this app produces.
pub fn spawn_remote_poller(
    app: AppHandle,
    tokens: TokenStore,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let api = WebApi::new();

        // Polls before the first sleep: opening the app while music plays on
        // another device must show it immediately, not after a delay.
        loop {
            let state = app.state::<AppState>();

            // Local playback is authoritative and event-driven; polling over
            // it would fight the event pump and waste quota.
            let skip = state.playback.read().await.is_active_device;
            let token = tokens.get().await;

            if skip || token.is_empty() {
                tokio::time::sleep(REMOTE_POLL).await;
                continue;
            }

            match connect::current_playback(&api, &token).await {
                Ok(remote) => {
                    if apply_remote(&state, remote).await {
                        let snap = snapshot(&state).await;
                        let _ = app.emit(events::PLAYBACK, &snap);
                    }
                }
                // Transient by nature — the next tick retries. Logged at debug
                // so a flaky network does not fill the log every 5s.
                Err(e) => log::debug!("remote playback poll failed: {e}"),
            }

            tokio::time::sleep(REMOTE_POLL).await;
        }
    })
}

/// Folds a `/me/player` response into `PlaybackState`. Returns whether
/// anything changed, so an unchanged poll emits no event.
async fn apply_remote(
    state: &tauri::State<'_, AppState>,
    remote: Option<connect::RemotePlayback>,
) -> bool {
    let mut pb = state.playback.write().await;
    let before = (pb.is_playing, pb.position_ms, pb.track.as_ref().map(|t| t.uri.clone()));

    let Some(r) = remote else {
        // 204: nothing playing anywhere.
        if pb.track.is_none() && !pb.is_playing {
            return false;
        }
        pb.is_playing = false;
        pb.track = None;
        pb.set_position(0);
        pb.duration_ms = 0;
        return true;
    };

    pb.is_active_device = false;
    pb.is_playing = r.is_playing;
    pb.set_position(r.progress_ms.unwrap_or(0));

    if let Some(shuffle) = r.shuffle_state {
        pb.shuffle = shuffle;
    }
    if let Some(repeat) = r.repeat_state.as_deref() {
        pb.repeat_context = repeat == "context";
        pb.repeat_track = repeat == "track";
    }
    if let Some(v) = r.device.as_ref().and_then(|d| d.volume_percent) {
        pb.volume = percent_to_volume(v);
    }

    if let Some(item) = r.item {
        pb.duration_ms = item.duration_ms;
        let cover = item
            .album
            .as_ref()
            .map(|a| a.images.as_slice())
            .filter(|i| !i.is_empty())
            .unwrap_or(item.images.as_slice())
            .iter()
            .min_by_key(|i| (i.width.unwrap_or(640) as i32 - 300).abs())
            .map(|i| i.url.clone());

        pb.track = Some(TrackInfo {
            uri: item.uri,
            name: item.name,
            artists: item.artists.into_iter().map(|a| a.name).collect(),
            album: item.album.map(|a| a.name).unwrap_or_default(),
            cover_url: cover,
            duration_ms: item.duration_ms,
        });
    }

    let after = (pb.is_playing, pb.position_ms, pb.track.as_ref().map(|t| t.uri.clone()));
    before != after
}

/// Rebuilds a fresh playback snapshot (used on reconnect / initial load).
pub async fn snapshot(state: &AppState) -> PlaybackState {
    // Write lock so the position can be advanced to now — a snapshot taken
    // mid-track must not report the position from the last event.
    let mut pb = state.playback.write().await;
    pb.refresh_position();
    pb.clone()
}
