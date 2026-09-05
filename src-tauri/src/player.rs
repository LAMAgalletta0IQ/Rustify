use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use librespot::connect::{ConnectConfig, LoadRequest, LoadRequestOptions, Spirc};
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
    generation: u64,
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
        log::info!("spirc task ended (session generation {generation})");
    });

    spawn_event_pump(app.clone(), event_rx, tokens, session.clone(), generation);

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
    generation: u64,
) {
    tauri::async_runtime::spawn(async move {
        let api = app.state::<AppState>().web_api.clone();
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
                    state.sleep_timer.observe_local_playback().await;
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
                    spawn_dj_advance(app.clone(), session.clone(), track_id.to_uri());
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

        // Reaching here means `rx.recv()` returned `None`: the sender inside
        // `Player` was dropped. librespot surfaces this to every subsequent
        // transport call as `Internal error { channel closed }`, and nothing
        // in the app rebuilt anything — the dead Spirc stayed installed in
        // `AppState::spotify`, so every play/pause/next failed identically
        // until the process was restarted. Hand off to the watchdog instead.
        log::warn!("player event pump ended (session generation {generation})");
        recover_closed_session(app, generation).await;
    });
}

/// How long to wait before each rebuild attempt after librespot's player
/// channel closes. Deliberately short at the front (a dropped socket usually
/// comes back at once) and capped, because every attempt spends a refresh
/// token round trip and Spotify rotates refresh tokens on use.
const RECOVERY_BACKOFF_SECS: [u64; 4] = [2, 6, 15, 45];

/// Rebuilds the librespot session after its player channel closed unexpectedly.
///
/// Deliberately does *not* sign the user out. The OAuth grant is still valid —
/// what died is the audio/Connect session on top of it — so the recovery path
/// is the same one `restore_session` uses at startup, and a failure leaves the
/// user logged in with a `Disconnected` status they can retry from rather than
/// bouncing them to the login screen.
async fn recover_closed_session(app: AppHandle, generation: u64) {
    let state = app.state::<AppState>();

    // A newer session already exists (a second login, or `establish` replacing
    // this one). This pump belongs to the old one; its channel closing is the
    // expected consequence of that replacement, not a fault.
    if state.session_generation() != generation {
        log::debug!("ignoring closed pump from superseded session {generation}");
        return;
    }
    if !state.auth.read().await.logged_in {
        return;
    }
    // One recovery at a time.
    if state
        .session_recovering
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }

    {
        let mut playback = state.playback.write().await;
        playback.connection_status = ConnectionStatus::Recovering;
        // The dead session cannot be the active Connect device any more, and
        // leaving the flag set would let transport buttons keep dispatching
        // into a Spirc that can no longer answer.
        playback.is_active_device = false;
        let snapshot = playback.clone();
        drop(playback);
        state.active_device.set(false);
        let _ = app.emit(events::PLAYBACK, snapshot);
    }

    let mut recovered = false;
    for (attempt, delay) in RECOVERY_BACKOFF_SECS.iter().enumerate() {
        tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
        // Re-check between attempts: the user may have signed out, or a
        // manual login may have built a healthy session while we waited.
        if state.session_generation() != generation || !state.auth.read().await.logged_in {
            recovered = true;
            break;
        }
        log::info!(
            "rebuilding playback session after channel closure (attempt {}/{})",
            attempt + 1,
            RECOVERY_BACKOFF_SECS.len()
        );
        match crate::commands::rebuild_session(&app).await {
            Ok(()) => {
                log::info!("playback session rebuilt");
                recovered = true;
                break;
            }
            Err(error) => log::warn!("playback session rebuild failed: {error}"),
        }
    }

    if !recovered {
        log::error!("giving up rebuilding the playback session; playback needs a manual retry");
        let snapshot = {
            let mut playback = state.playback.write().await;
            playback.connection_status = ConnectionStatus::Disconnected;
            playback.clone()
        };
        let _ = app.emit(events::PLAYBACK, snapshot);
    }

    state
        .session_recovering
        .store(false, std::sync::atomic::Ordering::SeqCst);
}

/// Drives DJ playback one track at a time: an outro for the track that just
/// ended (if it has one), Lexicon refill when the resolved track list is
/// running low, an intro for whichever track comes next (if it has one), then
/// the load that actually starts it.
///
/// DJ tracks are deliberately never queued into Spirc via `add_to_queue`
/// anymore (compare the previous, queue-based implementation this replaced):
/// playing narration between tracks means Rustify has to decide *when* the
/// next track starts, and Spirc's own queue auto-advance/crossfade would race
/// that decision — see `narration.rs`'s doc comment for why narration can't
/// simply be mixed into librespot's Sink chain instead. A side effect,
/// documented rather than silently accepted: DJ tracks no longer crossfade
/// into each other regardless of the crossfade setting (nothing is ever
/// queued for librespot's own crossfade to preload against), and Rustify's
/// queue view has nothing to show as "up next" during a DJ session, because
/// nothing actually is, in Spirc's own queue, until the moment it loads.
///
/// Only runs for a track from the active DJ session. Narration failures are
/// logged and skipped, never allowed to interrupt the music — matching
/// go-librespot's own "a failing narration clip is not a real error"
/// behavior for the same case.
fn spawn_dj_advance(app: AppHandle, session: Session, ended_uri: String) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let Some(mut dj) = state.internal_spotify.cached_dj().await else {
            return;
        };
        if !dj.active || !dj.dynamic_refill_supported {
            return;
        }
        let Some(ended_index) = dj.tracks.iter().position(|track| track.uri == ended_uri) else {
            return;
        };
        if !state.internal_spotify.begin_dj_refill() {
            return;
        }

        play_narration_if_present(&app, &session, dj.tracks[ended_index].clone(), "outro").await;

        // Keep enough lookahead that a refill always lands before it is
        // needed, mirroring the old queue-depth heuristic but against
        // Rustify's own resolved-track-list index rather than Spirc's queue.
        if dj.tracks.len().saturating_sub(ended_index) <= 8 {
            match state.internal_spotify.resolve_dj(&session, false).await {
                Ok(refreshed) => {
                    let known: std::collections::HashSet<_> =
                        dj.tracks.iter().map(|track| track.uri.clone()).collect();
                    dj.tracks
                        .extend(crate::spotify::dj_new_tracks(&known, refreshed.tracks));
                    state.internal_spotify.dj.set_cached(dj.clone()).await;
                }
                Err(error) => {
                    log::warn!(target: "spotify.dj", "dynamic session refill failed: {error}")
                }
            }
        }

        // Drop tracks that have already played: an open-ended DJ session
        // would otherwise grow this list for as long as the app keeps
        // running, for no benefit — nothing before the current track is ever
        // looked up again.
        if ended_index > 0 {
            dj.tracks.drain(..ended_index);
            state.internal_spotify.dj.set_cached(dj.clone()).await;
        }
        let ended_index = 0;

        state.internal_spotify.finish_dj_refill();

        let Some(next) = dj.tracks.get(ended_index + 1).cloned() else {
            log::debug!(target: "spotify.dj", "reached the end of the resolvable DJ session");
            state
                .internal_spotify
                .dj
                .update_status(false, dj.narration_resolved)
                .await;
            return;
        };

        play_narration_if_present(&app, &session, next.clone(), "intro").await;

        let load_result = {
            let spotify = state.spotify.read().await;
            match spotify.as_ref() {
                Some(spotify) => spotify.spirc.load(LoadRequest::from_tracks(
                    vec![next.uri.clone()],
                    LoadRequestOptions {
                        start_playing: true,
                        ..Default::default()
                    },
                )),
                None => return,
            }
        };
        if let Err(error) = load_result {
            log::warn!(target: "spotify.dj", "failed to advance to next DJ track: {error}");
            return;
        }
        state
            .internal_spotify
            .dj
            .update_status(true, dj.narration_resolved)
            .await;
    });
}

/// Plays a DJ track's intro narration clip, if it has one. Public entry point
/// for `commands::discovery::start_dj`, which controls the very first DJ
/// track's load directly rather than through `spawn_dj_advance` (there is no
/// "previous track" to fire an `EndOfTrack` and trigger it otherwise).
pub(crate) async fn play_dj_intro(
    app: &AppHandle,
    session: &Session,
    track: &crate::spotify::DjTrack,
) {
    play_narration_if_present(app, session, track.clone(), "intro").await;
}

/// Resolves, fetches, decodes and plays one track's narration clip of the
/// given kind ("intro" or "outro"), if it has a script for it. Every failure
/// mode — no script, resolution error, fetch error, decode error, playback
/// error — is a no-op: narration is a nice-to-have around the music, never a
/// gate on it playing.
async fn play_narration_if_present(
    app: &AppHandle,
    session: &Session,
    track: crate::spotify::DjTrack,
    kind: &str,
) {
    if !track.narration_kinds.iter().any(|k| k == kind) {
        return;
    }
    let state = app.state::<AppState>();
    let resolved = match state
        .internal_spotify
        .resolve_dj_narration(session, &track, kind)
        .await
    {
        Ok(Some(resolved)) => resolved,
        Ok(None) => return,
        Err(error) => {
            log::warn!(target: "spotify.dj", "{kind} narration resolution failed, skipping it: {error}");
            return;
        }
    };
    let bytes = match crate::narration::fetch_clip(
        state.internal_spotify.dj.audio_client(),
        &resolved.url,
    )
    .await
    {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!(target: "spotify.dj", "{kind} narration fetch failed, skipping it: {error}");
            return;
        }
    };
    let gain = crate::narration::narration_gain(resolved.loudness_db, resolved.true_peak_db, 0.0);
    let samples = match crate::narration::decode_clip(bytes, gain) {
        Ok(samples) => samples,
        Err(error) => {
            log::warn!(target: "spotify.dj", "{kind} narration decode failed, skipping it: {error}");
            return;
        }
    };
    if let Err(error) = crate::narration::play_clip(&state.audio, samples).await {
        log::warn!(target: "spotify.dj", "{kind} narration playback failed: {error}");
    }
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

        // A crossfade from the track that just ended can still be mixing its
        // tail under this episode's opening seconds — librespot's crossfade is
        // a fixed-length decoder overlap that runs independently of Rustify's
        // own event timing, so it does not know a resume-seek is about to move
        // the target track out from under it. Bracket the seek exactly like the
        // manual `seek` command does (see `commands::clear_crossfade_before_transition`),
        // or the outgoing track's tail keeps blending under audio from the
        // wrong position.
        let resume_after = crate::commands::clear_crossfade_before_transition(&app, &state)
            .await
            .unwrap_or(false);

        let position = resume.position_ms.min(u64::from(u32::MAX)) as u32;
        let seek_result =
            crate::commands::with_spirc(&state, |s| s.set_position_ms(position)).await;
        match seek_result {
            Ok(()) => log::debug!(target: "spotify.podcasts", "resumed {uri} at {position}ms"),
            Err(error) => {
                log::debug!(target: "spotify.podcasts", "automatic resume seek failed for {uri}: {error}")
            }
        }
        if resume_after {
            if let Err(error) = crate::commands::with_spirc(&state, |s| s.play()).await {
                log::debug!(target: "spotify.podcasts", "automatic resume-seek could not restore playback for {uri}: {error}");
            }
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
    id: String,
    uri: String,
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
                album_id: None,
                album_uri: None,
                cover_url: pick_cover(&ep.images),
                duration_ms: ep.duration_ms,
            })
        }
        _ => {
            let tr: ApiTrack = api.get(token, &format!("/tracks/{id}"), &[]).await?;
            let album_id = tr.album.id.clone();
            let album_uri = tr.album.uri.clone();
            Ok(TrackInfo {
                uri: uri_str,
                name: tr.name,
                artists: tr.artists.into_iter().map(|a| a.name).collect(),
                album: tr.album.name,
                album_id: Some(album_id),
                album_uri: Some(album_uri),
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
        let api = app.state::<AppState>().web_api.clone();
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
                    let changed = apply_remote(&state, remote).await;
                    let snap = snapshot(&state).await;
                    state.sleep_timer.observe_remote_playback(&app, &snap).await;
                    if changed {
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

        let album = item.album;
        let album_name = album
            .as_ref()
            .map(|value| value.name.clone())
            .unwrap_or_default();
        let album_id = album.as_ref().map(|value| value.id.clone());
        let album_uri = album.as_ref().map(|value| value.uri.clone());
        pb.track = Some(TrackInfo {
            uri: item.uri,
            name: item.name,
            artists: item.artists.into_iter().map(|a| a.name).collect(),
            album: album_name,
            album_id,
            album_uri,
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
