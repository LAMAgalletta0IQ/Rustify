use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use librespot::connect::{ConnectConfig, Spirc};
use librespot::core::authentication::Credentials;
use librespot::core::cache::Cache;
use librespot::core::config::{DeviceType, SessionConfig};
use librespot::core::session::Session;
use librespot::core::SpotifyUri;
use librespot::playback::audio_backend;
use librespot::playback::config::{AudioFormat, PlayerConfig};
use librespot::playback::mixer::{self, MixerConfig};
use librespot::playback::player::{Player, PlayerEvent};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use crate::audio::{self, StreamQuality};
use crate::connect;
use crate::error::{AppError, AppResult};
use crate::state::{events, AppState, ConnectionStatus, PlaybackState, TokenStore, TrackInfo};
use crate::webapi::WebApi;

/// librespot's internal volume scale.
pub const MAX_VOLUME: u16 = u16::MAX;

pub fn percent_to_volume(percent: u8) -> u16 {
    ((percent.min(100) as u32 * MAX_VOLUME as u32) / 100) as u16
}

fn playback_config(quality: StreamQuality, crossfade_seconds: u8) -> PlayerConfig {
    PlayerConfig {
        bitrate: quality.bitrate(),
        // Besides keeping long-form UI state fresh, this gives podcast
        // resumption a bounded one-minute checkpoint if the process exits
        // without a normal pause/unload event.
        position_update_interval: Some(std::time::Duration::from_secs(60)),
        // The player owns both decoders, so overlap happens before Rustify's
        // output-device/EQ sink and remains a single continuous sink stream.
        // PlayerConfig's default keeps gapless playback enabled; crossfade
        // depends on that continuous sink and must not regress it when off.
        crossfade: std::time::Duration::from_secs(crossfade_seconds.into()),
        ..Default::default()
    }
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
    options: PlaybackOptions,
) -> AppResult<StartedSession> {
    let PlaybackOptions {
        initial_volume_percent,
        cache_limit_mb,
        quality,
        crossfade_seconds,
    } = options;
    let session_config = SessionConfig::default();
    let player_config = playback_config(quality, crossfade_seconds);
    let audio_format = AudioFormat::default();
    let mixer_config = MixerConfig::default();

    let connect_config = ConnectConfig {
        name: device_name,
        device_type: DeviceType::Computer,
        // Raw 0..=u16::MAX scale, NOT a percentage — librespot's own default
        // is u16::MAX / 2. Passing 50 here yields ~0.08% volume, i.e. silence.
        initial_volume: percent_to_volume(initial_volume_percent),
        // Local loads, remote set_queue commands and queue additions are
        // projected immediately instead of waiting for the next cluster push.
        emit_set_queue_events: true,
        ..Default::default()
    };

    // Audio file cache keeps re-listens off the network; credentials cache lets
    // librespot reconnect without another OAuth round-trip.
    let cache = Cache::new(
        Some(cache_dir.as_path()),
        Some(cache_dir.as_path()),
        Some(cache_dir.join("files").as_path()),
        // Cap the audio cache so it cannot grow without bound on a small disk.
        Some(cache_limit_mb as u64 * 1024 * 1024),
    )?;

    let sink_builder = audio_backend::find(None)
        .ok_or_else(|| AppError::Playback("no audio backend available".into()))?;
    let mixer_builder =
        mixer::find(None).ok_or_else(|| AppError::Playback("no mixer available".into()))?;

    let session = Session::new(session_config, Some(cache));
    let mixer = mixer_builder(mixer_config)?;

    let audio_runtime = app.state::<AppState>().audio.clone();
    let player = Player::new(
        player_config,
        session.clone(),
        mixer.get_soft_volume(),
        move || audio::processing_sink(sink_builder, audio_format, audio_runtime),
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

    spawn_event_pump(app.clone(), event_rx, tokens, session.clone());

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
        let mut playback = state.playback.write().await;
        playback.volume = mixer.volume();
        playback.audio_quality = quality;
        playback.audio_quality_label = quality.effective_label().to_string();
    }

    Ok(StartedSession { session, spirc })
}

/// What `start_session` hands back; the caller pairs it with the refresh task
/// to form a [`SpotifySession`].
pub struct PlaybackOptions {
    pub initial_volume_percent: u8,
    pub cache_limit_mb: u32,
    pub quality: StreamQuality,
    pub crossfade_seconds: u8,
}

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
    session: Session,
) {
    tauri::async_runtime::spawn(async move {
        let api = WebApi::new();
        let cache: Arc<Mutex<HashMap<String, TrackInfo>>> = Arc::new(Mutex::new(HashMap::new()));

        while let Some(event) = rx.recv().await {
            let state = app.state::<AppState>();
            state.telemetry.observe(&event).await;

            if let PlayerEvent::SetQueue {
                context_uri,
                current_track,
                next_tracks,
                prev_tracks,
            } = &event
            {
                let projected = {
                    let existing = state.queue.read().await;
                    crate::queue::from_player_event(
                        current_track.as_ref(),
                        next_tracks,
                        prev_tracks,
                        &existing,
                    )
                };
                *state.queue.write().await = projected.clone();
                if !context_uri.is_empty() && context_uri != "-" {
                    state.playback.write().await.context_uri = Some(context_uri.clone());
                }
                let _ = app.emit(events::QUEUE, projected);
                continue;
            }
            let mut pb = state.playback.write().await;

            let mut track_to_resolve: Option<SpotifyUri> = None;
            let mut resume_report: Option<(String, u64)> = None;
            let mut resume_lookup: Option<String> = None;

            match event {
                PlayerEvent::SessionConnected { .. } => {
                    // Fired the moment `Spirc::activate` succeeds — the
                    // authoritative "we are now the active device" signal,
                    // ahead of any audio actually playing.
                    pb.is_active_device = true;
                    state.active_device.set(true);
                    pb.connection_status = ConnectionStatus::Connected;
                }
                PlayerEvent::Playing {
                    track_id,
                    position_ms,
                    ..
                } => {
                    pb.is_playing = true;
                    pb.is_loading = false;
                    pb.is_active_device = true;
                    state.active_device.set(true);
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
                    state.active_device.set(true);
                    pb.set_position(position_ms);
                    if track_id.item_type() == "episode" && position_ms > 0 {
                        resume_report = Some((track_id.to_uri(), u64::from(position_ms)));
                    }
                    track_to_resolve = Some(track_id);
                }
                PlayerEvent::Loading {
                    track_id,
                    position_ms,
                    ..
                } => {
                    pb.is_loading = true;
                    pb.is_active_device = true;
                    state.active_device.set(true);
                    pb.set_position(position_ms);
                    if track_id.item_type() == "episode" && position_ms == 0 {
                        resume_lookup = Some(track_id.to_uri());
                    }
                    track_to_resolve = Some(track_id);
                }
                PlayerEvent::Stopped { track_id, .. } => {
                    pb.refresh_position();
                    if track_id.item_type() == "episode" && pb.position_ms > 0 {
                        resume_report = Some((track_id.to_uri(), u64::from(pb.position_ms)));
                    }
                    pb.is_playing = false;
                    pb.is_loading = false;
                    pb.set_position(0);
                }
                PlayerEvent::PositionCorrection { position_ms, .. } => {
                    pb.set_position(position_ms);
                }
                PlayerEvent::PositionChanged {
                    track_id,
                    position_ms,
                    ..
                }
                | PlayerEvent::Seeked {
                    track_id,
                    position_ms,
                    ..
                } => {
                    pb.set_position(position_ms);
                    if track_id.item_type() == "episode" && position_ms > 0 {
                        resume_report = Some((track_id.to_uri(), u64::from(position_ms)));
                    }
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
                    // Spirc emits this when a Connect cluster update shows a
                    // different device took over as active — see
                    // `ActiveDeviceSignal`'s doc comment for the code path.
                    pb.refresh_position();
                    if let Some(track) = pb
                        .track
                        .as_ref()
                        .filter(|track| track.uri.starts_with("spotify:episode:"))
                    {
                        resume_report = Some((track.uri.clone(), u64::from(pb.position_ms)));
                    }
                    pb.is_active_device = false;
                    pb.is_playing = false;
                    state.active_device.set(false);
                }
                PlayerEvent::EndOfTrack { track_id, .. } => {
                    drop(pb);
                    if track_id.item_type() == "episode" {
                        crate::podcasts::spawn_report(session.clone(), track_id.to_uri(), 0, true);
                    }
                    spawn_dj_refill(app.clone(), session.clone(), track_id.to_uri());
                    state.sleep_timer.on_end_of_track(&app).await;
                    continue;
                }
                // Preload/PlayRequestIdChanged and the remaining
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
                let uri_str = uri.to_uri();
                let changed = pb.track.as_ref().map(|t| t.uri != uri_str).unwrap_or(true);

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
            if let Some((uri, position_ms)) = resume_report {
                crate::podcasts::spawn_report(session.clone(), uri, position_ms, false);
            }
            if let Some(uri) = resume_lookup {
                spawn_resume_lookup(app.clone(), session.clone(), uri);
            }
        }

        log::info!("player event pump ended");
    });
}

/// Best-effort compatibility bridge until librespot itself understands
/// Lexicon/hm:// dynamic contexts. It only runs for a track from the active DJ
/// window and only when the observable queue is low. Failures never interrupt
/// music playback.
fn spawn_dj_refill(app: AppHandle, session: Session, ended_uri: String) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Some(previous) = state.internal_spotify.cached_dj().await else {
            return;
        };
        if !previous.active || !previous.tracks.iter().any(|track| track.uri == ended_uri) {
            return;
        }
        let remaining = {
            let queue = state.queue.read().await;
            queue.queue.len() + queue.autoplay.len()
        };
        if remaining >= 8 || !state.internal_spotify.begin_dj_refill() {
            return;
        }

        let outcome = async {
            let refreshed = state.internal_spotify.resolve_dj(&session, false).await?;
            let queued = {
                let queue = state.queue.read().await;
                queue
                    .queue
                    .iter()
                    .chain(&queue.autoplay)
                    .map(|track| track.uri.clone())
                    .collect::<Vec<_>>()
            };
            let additions: Vec<_> =
                crate::spotify::dj_refill_uris(&previous, &refreshed, queued, 32)
                    .iter()
                    .filter_map(|uri| SpotifyUri::from_uri(uri).ok())
                    .collect();
            if additions.is_empty() {
                return Ok::<usize, AppError>(0);
            }
            let spotify = state.spotify.read().await;
            let spotify = spotify.as_ref().ok_or(AppError::NotLoggedIn)?;
            for uri in &additions {
                spotify.spirc.add_to_queue(uri.clone())?;
            }
            Ok(additions.len())
        }
        .await;

        state.internal_spotify.finish_dj_refill();
        state
            .internal_spotify
            .dj
            .update_status(true, previous.narration_resolved)
            .await;
        match outcome {
            Ok(0) => log::debug!(target: "spotify.dj", "Lexicon refill returned no new tracks"),
            Ok(count) => {
                log::debug!(target: "spotify.dj", "added {count} refreshed Lexicon tracks")
            }
            Err(error) => log::warn!(target: "spotify.dj", "dynamic queue refill failed: {error}"),
        }
    });
}

fn spawn_resume_lookup(app: AppHandle, session: Session, uri: String) {
    tauri::async_runtime::spawn(async move {
        let resume = match crate::podcasts::get(&session, &uri).await {
            Ok(resume) if !resume.completed && resume.position_ms > 0 => resume,
            Ok(_) => return,
            Err(error) => {
                log::debug!(target: "spotify.podcasts", "automatic resume lookup failed for {uri}: {error}");
                return;
            }
        };
        let state = app.state::<AppState>();
        let still_current = {
            let playback = state.playback.read().await;
            playback.is_active_device
                && playback
                    .track
                    .as_ref()
                    .is_some_and(|track| track.uri == uri)
                && playback.position_ms <= 2_000
        };
        if !still_current {
            return;
        }
        let spotify = state.spotify.read().await;
        let Some(spotify) = spotify.as_ref() else {
            return;
        };
        let position = resume.position_ms.min(u64::from(u32::MAX)) as u32;
        if let Err(error) = spotify.spirc.set_position_ms(position) {
            log::debug!(target: "spotify.podcasts", "automatic resume seek failed for {uri}: {error}");
        } else {
            log::debug!(target: "spotify.podcasts", "resumed {uri} at {position}ms");
        }
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
    let id = uri.to_id();
    let uri_str = uri.to_uri();

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
const REMOTE_POLL: std::time::Duration = std::time::Duration::from_secs(30);

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
        let mut active_rx = app.state::<AppState>().active_device.subscribe();

        // Polls before the first sleep: opening the app while music plays on
        // another device must show it immediately, not after a delay.
        loop {
            let state = app.state::<AppState>();

            // Local playback is authoritative and event-driven; polling over
            // it would fight the event pump and waste quota. `active_device`
            // is the canonical signal (see `ActiveDeviceSignal`), not the
            // `PlaybackState` copy, though the two are always in sync.
            let skip = *active_rx.borrow_and_update();
            let token = tokens.get().await;

            if skip || token.is_empty() {
                tokio::select! {
                    _ = tokio::time::sleep(REMOTE_POLL) => {}
                    changed = active_rx.changed() => if changed.is_err() { break },
                }
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

            tokio::select! {
                _ = tokio::time::sleep(REMOTE_POLL) => {}
                changed = active_rx.changed() => if changed.is_err() { break },
            }
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
    let before = remote_identity(&pb);

    let Some(r) = remote else {
        // 204: nothing playing anywhere.
        if pb.track.is_none()
            && !pb.is_playing
            && pb.context_uri.is_none()
            && pb.active_device.is_none()
        {
            return false;
        }
        pb.is_playing = false;
        pb.is_loading = false;
        pb.is_active_device = false;
        pb.track = None;
        pb.set_position(0);
        pb.duration_ms = 0;
        pb.context_uri = None;
        pb.active_device = None;
        for device in &mut pb.available_devices {
            device.is_active = false;
        }
        state.active_device.set(false);
        return true;
    };

    pb.is_active_device = false;
    pb.connection_status = ConnectionStatus::Connected;
    state.active_device.set(false);
    pb.is_playing = r.is_playing;
    pb.is_loading = false;

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
    pb.active_device = r.device.clone();
    if let Some(device) = r.device {
        if let Some(existing) = pb
            .available_devices
            .iter_mut()
            .find(|candidate| candidate.id == device.id)
        {
            *existing = device;
        } else {
            pb.available_devices.push(device);
        }
    }
    pb.context_uri = r
        .context
        .and_then(|context| (!context.uri.is_empty() && context.uri != "-").then_some(context.uri));

    if let Some(item) = r.item {
        pb.duration_ms = item.duration_ms;
        pb.set_position(r.progress_ms.unwrap_or(0).min(item.duration_ms));
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
    } else {
        pb.set_position(r.progress_ms.unwrap_or(0));
        pb.duration_ms = 0;
        pb.track = None;
    }

    let after = remote_identity(&pb);
    before != after
}

#[derive(PartialEq, Eq)]
struct RemoteIdentity {
    playing: bool,
    loading: bool,
    position: u32,
    duration: u32,
    track: Option<TrackInfo>,
    context: Option<String>,
    volume: u16,
    shuffle: bool,
    repeat_context: bool,
    repeat_track: bool,
    active_device: Option<connect::Device>,
}

fn remote_identity(playback: &PlaybackState) -> RemoteIdentity {
    RemoteIdentity {
        playing: playback.is_playing,
        loading: playback.is_loading,
        position: playback.position_ms,
        duration: playback.duration_ms,
        track: playback.track.clone(),
        context: playback.context_uri.clone(),
        volume: playback.volume,
        shuffle: playback.shuffle,
        repeat_context: playback.repeat_context,
        repeat_track: playback.repeat_track,
        active_device: playback.active_device.clone(),
    }
}

/// Rebuilds a fresh playback snapshot (used on reconnect / initial load).
pub async fn snapshot(state: &AppState) -> PlaybackState {
    // Write lock so the position can be advanced to now — a snapshot taken
    // mid-track must not report the position from the last event.
    let mut pb = state.playback.write().await;
    pb.refresh_position();
    pb.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfade_enables_gapless_and_reaches_decoder_config() {
        let disabled = playback_config(StreamQuality::Normal, 0);
        assert!(disabled.crossfade.is_zero());
        assert!(disabled.gapless);

        let enabled = playback_config(StreamQuality::VeryHigh, 7);
        assert_eq!(enabled.crossfade, std::time::Duration::from_secs(7));
        assert!(enabled.gapless);
    }

    #[test]
    fn remote_identity_detects_device_only_changes() {
        let mut playback = PlaybackState {
            active_device: Some(connect::Device {
                id: Some("phone".into()),
                name: "Phone".into(),
                device_type: "smartphone".into(),
                is_active: true,
                is_restricted: false,
                volume_percent: Some(20),
            }),
            ..Default::default()
        };
        let before = remote_identity(&playback);
        playback.active_device.as_mut().unwrap().volume_percent = Some(80);
        assert!(before != remote_identity(&playback));
    }
}
