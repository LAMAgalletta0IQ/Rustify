use std::sync::Arc;

use librespot::connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
use librespot::core::authentication::Credentials;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::audio::{
    self, AudioDevice, AudioStatus, EqualizerPreset, EqualizerSettings, StreamQuality,
};
use crate::auth;
use crate::connect::{self, Device};
use crate::error::{AppError, AppResult};
use crate::jams::JamSession;
use crate::jams_bridge::JamController;
use crate::library::{
    self, AlbumPage, AlbumSummary, ArtistPage, FollowedReleasePage, PlaylistSummary,
    RecentActivityItem, TrackSummary,
};
use crate::lyrics::LyricsResult;
use crate::player;
use crate::queue::{self, QueueView};
use crate::search::ArtistSummary;
use crate::search::{self, SearchResults};
use crate::spotify::{DjSession, HomeFeed};
use crate::state::{events, AppState, AuthState, ConnectionStatus, PlaybackState, SpotifySession};
use crate::webapi::WebApi;

/// Pulls the Web API bearer token out of the live session, or fails cleanly if
/// the user is not logged in. Every Web API command starts here.
async fn token(state: &AppState) -> AppResult<String> {
    if state.spotify.read().await.is_none() {
        return Err(AppError::NotLoggedIn);
    }
    let token = state.tokens.get().await;
    if token.is_empty() {
        return Err(AppError::SessionExpired);
    }
    Ok(token)
}

/// Runs `f` against the live Spirc handle.
async fn with_spirc<F>(state: &AppState, f: F) -> AppResult<()>
where
    F: FnOnce(&librespot::connect::Spirc) -> Result<(), librespot::core::Error>,
{
    let guard = state.spotify.read().await;
    let s = guard.as_ref().ok_or(AppError::NotLoggedIn)?;
    f(&s.spirc).map_err(AppError::from)
}

async fn remote_put(state: &AppState, path: &str, query: &[(&str, String)]) -> AppResult<()> {
    WebApi::new()
        .put_query(&token(state).await?, path, query)
        .await
}

async fn remote_post(state: &AppState, path: &str, query: &[(&str, String)]) -> AppResult<()> {
    WebApi::new()
        .post_query(&token(state).await?, path, query)
        .await
}

fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| format!("{s} (Rustify)"))
        .unwrap_or_else(|| "Rustify".to_string())
}

// ---- auth ---------------------------------------------------------------

#[tauri::command]
pub async fn get_auth_state(state: State<'_, AppState>) -> AppResult<AuthState> {
    Ok(state.auth.read().await.clone())
}

/// Returns the authenticated account's actual Web Player Home shelves: Daily
/// Mixes, Discover Weekly, Release Radar, daylist and other personalized
/// contexts where Spotify exposes them. This uses semantic card metadata and
/// never infers a feature from its localized display name.
#[tauri::command]
pub async fn get_personalized_home(
    state: State<'_, AppState>,
    limit: Option<u32>,
    time_zone: Option<String>,
) -> AppResult<HomeFeed> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    state
        .internal_spotify
        .home(
            &session,
            limit.unwrap_or(10),
            time_zone.as_deref().unwrap_or("UTC"),
        )
        .await
}

#[tauri::command]
pub async fn get_dj_status(
    state: State<'_, AppState>,
    refresh: Option<bool>,
) -> AppResult<Option<DjSession>> {
    if !refresh.unwrap_or(false) {
        if let Some(session) = state.internal_spotify.cached_dj().await {
            return Ok(Some(session));
        }
    }
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    state
        .internal_spotify
        .resolve_dj(&session, false)
        .await
        .map(Some)
}

/// Resolves the dynamic DJ session through Lexicon before loading its current
/// track window. librespot 0.8 cannot resolve empty dynamic context pages on
/// its own, so passing the recovered URIs is the compatibility path.
#[tauri::command]
pub async fn start_dj(state: State<'_, AppState>) -> AppResult<DjSession> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    let mut dj = state.internal_spotify.resolve_dj(&session, false).await?;
    match state
        .internal_spotify
        .prepare_dj_narration(&session, &dj)
        .await
    {
        Ok(resolved) => dj.narration_resolved = resolved,
        Err(error) => {
            log::warn!(target: "spotify.dj", "narration preflight failed; continuing with music: {error}")
        }
    }
    let uris: Vec<_> = dj.tracks.iter().map(|track| track.uri.clone()).collect();
    let first = uris.first().cloned();
    let activate = !state.playback.read().await.is_active_device;
    with_spirc(&state, move |spirc| {
        if activate {
            spirc.activate()?;
        }
        spirc.load(LoadRequest::from_tracks(
            uris,
            LoadRequestOptions {
                start_playing: true,
                playing_track: first.map(PlayingTrack::Uri),
                ..Default::default()
            },
        ))
    })
    .await?;
    state
        .internal_spotify
        .dj
        .update_status(true, dj.narration_resolved)
        .await;
    dj.active = true;
    Ok(dj)
}

/// Shape of the login flow, so the UI can describe it accurately instead of
/// guessing, and so it knows whether the first-run Setup screen is needed.
/// Read at render time, not cached, since it reflects whatever was last saved
/// through `set_client_id` (or the `.env` override).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    /// True once a Web API client ID is configured (env override, or saved via
    /// `set_client_id`). False means Setup must run before Login can.
    pub private_client_id: bool,
    /// Name of the variable to set, so the UI never hardcodes it.
    pub client_id_env: &'static str,
    /// Redirect URI the user must register against their own app.
    pub webapi_redirect_uri: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_volume_percent: u8,
    pub reduce_motion: bool,
    pub cache_limit_mb: u32,
    pub audio_quality: StreamQuality,
    pub crossfade_seconds: u8,
    pub output_device: Option<String>,
    pub equalizer: EqualizerSettings,
}

fn validate_crossfade(seconds: u8) -> AppResult<()> {
    if seconds <= 12 {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "Crossfade must be between 0 and 12 seconds.".into(),
        ))
    }
}

impl From<auth::Settings> for AppSettings {
    fn from(value: auth::Settings) -> Self {
        Self {
            default_volume_percent: value.default_volume_percent,
            reduce_motion: value.reduce_motion,
            cache_limit_mb: value.cache_limit_mb,
            audio_quality: value.audio_quality,
            crossfade_seconds: value.crossfade_seconds,
            output_device: value.output_device,
            equalizer: value.equalizer,
        }
    }
}

#[tauri::command]
pub fn get_login_info(app: AppHandle) -> AppResult<LoginInfo> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    Ok(LoginInfo {
        private_client_id: auth::webapi_client_id(&data_dir).is_ok(),
        client_id_env: auth::CLIENT_ID_ENV,
        webapi_redirect_uri: auth::webapi_redirect_uri(),
    })
}

/// Saves the Web API client ID from the first-run Setup screen (or a later
/// correction via "Use a different Client ID" on the Login screen).
#[tauri::command]
pub fn set_client_id(app: AppHandle, client_id: String) -> AppResult<()> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let trimmed = client_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::Other("Client ID cannot be empty.".to_string()));
    }

    let mut settings = auth::settings_or_default(&data_dir);
    settings.webapi_client_id = Some(trimmed.to_string());
    auth::save_settings(&data_dir, &settings)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> AppResult<AppSettings> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;
    Ok(auth::settings_or_default(&data_dir).into())
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    if settings.default_volume_percent > 100 {
        return Err(AppError::BadRequest(
            "Default volume must be between 0 and 100.".into(),
        ));
    }
    if !(128..=8192).contains(&settings.cache_limit_mb) {
        return Err(AppError::BadRequest(
            "Cache size must be between 128 MB and 8192 MB.".into(),
        ));
    }
    validate_crossfade(settings.crossfade_seconds)?;
    audio::validate_equalizer(&settings.equalizer)?;
    audio::validate_output_device(settings.output_device.as_deref())?;

    let mut persisted = auth::settings_or_default(&data_dir);
    persisted.default_volume_percent = settings.default_volume_percent;
    persisted.reduce_motion = settings.reduce_motion;
    persisted.cache_limit_mb = settings.cache_limit_mb;
    persisted.audio_quality = settings.audio_quality;
    persisted.crossfade_seconds = settings.crossfade_seconds;
    persisted.output_device = settings.output_device;
    persisted.equalizer = settings.equalizer;
    auth::save_settings(&data_dir, &persisted)?;
    state
        .audio
        .configure(persisted.output_device.clone(), persisted.equalizer.clone());
    Ok(persisted.into())
}

/// Enumerating devices can touch OS audio services, so callers run this command
/// off the webview thread through Tauri's async command dispatcher.
#[tauri::command]
pub async fn list_audio_devices(state: State<'_, AppState>) -> AppResult<Vec<AudioDevice>> {
    let runtime = state.audio.clone();
    tauri::async_runtime::spawn_blocking(move || audio::list_output_devices(&runtime))
        .await
        .map_err(|e| AppError::Other(format!("audio device enumeration failed: {e}")))?
}

#[tauri::command]
pub fn get_audio_status(state: State<'_, AppState>) -> AudioStatus {
    state.audio.status()
}

#[tauri::command]
pub fn get_equalizer_presets() -> Vec<EqualizerPreset> {
    audio::builtin_presets()
}

/// Completes a login given an OAuth token: verifies Premium, starts librespot,
/// persists the refresh token.
async fn establish(
    app: &AppHandle,
    state: &AppState,
    api: &WebApi,
    toks: auth::SessionTokens,
) -> AppResult<AuthState> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    // Persist the refresh tokens *before* the Premium gate. The OAuth flow has
    // already succeeded by this point, so if the gate then fails on a
    // transient error (notably a 429 on /me), a retry can go through
    // `restore_session` silently instead of reopening the browser.
    let stored = toks.stored();
    // Which halves actually made it to disk. Without this, a Web API refresh
    // token that never persists is invisible until the *next* launch falls
    // back to the shared quota and starts collecting 429s.
    log::info!(
        "persisting tokens: streaming={}, web api={} (split={})",
        !stored.refresh_token.trim().is_empty(),
        stored.webapi_refresh_token.is_some(),
        toks.webapi_client_id != auth::streaming_client_id(),
    );
    auth::save_stored_tokens(&data_dir, &stored)?;

    let settings = auth::settings_or_default(&data_dir);
    audio::validate_equalizer(&settings.equalizer)?;
    state
        .audio
        .configure(settings.output_device.clone(), settings.equalizer.clone());

    // Premium gate, so a free account gets a clear message rather than a
    // silent playback failure later. Uses the Web API token, which is the one
    // authorised for `/me`.
    let auth_state = auth::fetch_profile_require_premium(api, &toks.webapi.access_token).await?;

    // Publish the token before anything reads it.
    state.tokens.set(toks.webapi.access_token.clone()).await;
    state.playback.write().await.connection_status = ConnectionStatus::Connecting;

    // librespot gets the *streaming* bearer, which is the one carrying the
    // `streaming` scope from Spotify's desktop client ID.
    let started = player::start_session(
        app.clone(),
        Credentials::with_access_token(toks.streaming_access.clone()),
        state.tokens.clone(),
        device_name(),
        data_dir.join("cache"),
        player::PlaybackOptions {
            initial_volume_percent: settings.default_volume_percent,
            cache_limit_mb: settings.cache_limit_mb,
            quality: settings.audio_quality,
            crossfade_seconds: settings.crossfade_seconds,
        },
    )
    .await?;

    // Access tokens last ~1h; without this every Web API call would start
    // failing mid-session.
    let refresh_task = auth::spawn_refresher(
        app.clone(),
        toks.webapi_client_id.clone(),
        stored,
        state.tokens.clone(),
        data_dir,
        toks.webapi.expires_at,
    );

    // Replacing the session must tear the old one down explicitly. Dropping a
    // tokio JoinHandle *detaches* the task rather than cancelling it, so a
    // second `establish` without a logout would leave the previous refresher
    // running — two tasks then hit the token endpoint on their own schedules,
    // which is exactly how a client walks itself into a 429.
    if let Some(old) = state.spotify.write().await.take() {
        log::warn!("replacing an existing session; shutting the old one down");
        let _ = old.spirc.shutdown();
        old.refresh_task.abort();
        old.remote_task.abort();
        old.connect_state_task.abort();
        if let Some(task) = old.friends_task {
            task.abort();
        }
    }

    // Reflects playback on the user's other devices, so opening the app while
    // listening on a phone shows the current track instead of "Nothing playing".
    let remote_task = player::spawn_remote_poller(app.clone(), state.tokens.clone());
    let connect_state_task = match crate::remote_state::spawn(app.clone(), started.session.clone())
    {
        Ok(task) => task,
        Err(error) => {
            remote_task.abort();
            refresh_task.abort();
            let _ = started.spirc.shutdown();
            state.playback.write().await.connection_status = ConnectionStatus::Disconnected;
            return Err(error);
        }
    };
    *state.friend_activity.write().await = crate::friends::FriendFeed::default();
    let friends_task = match crate::friends::spawn(app.clone(), started.session.clone()) {
        Ok(task) => Some(task),
        Err(error) => {
            // Friend presence is an optional capability. A changed/disabled
            // endpoint must not take down authentication or playback.
            log::debug!(target: "spotify.social", "friend activity unavailable: {error}");
            *state.friend_activity.write().await = crate::friends::FriendFeed {
                status: crate::friends::FriendFeedStatus::Unavailable,
                available: false,
                entries: Vec::new(),
                updated_at_ms: None,
            };
            None
        }
    };

    state.playback.write().await.connection_status = ConnectionStatus::Connected;

    *state.spotify.write().await = Some(SpotifySession {
        session: started.session,
        spirc: started.spirc,
        refresh_task,
        remote_task,
        connect_state_task,
        friends_task,
    });
    *state.auth.write().await = auth_state.clone();

    let _ = app.emit(events::AUTH, &auth_state);
    Ok(auth_state)
}

#[tauri::command]
pub async fn login(app: AppHandle, state: State<'_, AppState>) -> AppResult<AuthState> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let api = WebApi::new();
    let toks = auth::interactive_login(&data_dir).await?;
    establish(&app, &state, &api, toks).await
}

#[tauri::command]
pub async fn start_device_authorization(
    state: State<'_, AppState>,
) -> AppResult<auth::DeviceAuthorization> {
    auth::start_device_authorization(&state.device_auth).await
}

#[tauri::command]
pub async fn complete_device_authorization(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<AuthState> {
    let api = WebApi::new();
    let tokens = auth::complete_device_authorization(&state.device_auth).await?;
    establish(&app, &state, &api, tokens).await
}

#[tauri::command]
pub async fn cancel_device_authorization(state: State<'_, AppState>) -> AppResult<()> {
    state.device_auth.cancel().await;
    Ok(())
}

/// Attempted once at startup. Returns a logged-out state rather than an error
/// when there is nothing stored, so the UI can just show the login screen.
#[tauri::command]
pub async fn restore_session(app: AppHandle, state: State<'_, AppState>) -> AppResult<AuthState> {
    // The webview can reload while the Rust process and librespot session stay
    // alive. Reusing it avoids concurrent refresh-token rotations and the
    // resulting invalid-grant/login loop.
    if state.spotify.read().await.is_some() {
        let current = state.auth.read().await.clone();
        if current.logged_in {
            return Ok(current);
        }
    }

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let Some(stored) = auth::load_stored_tokens(&data_dir) else {
        return Ok(AuthState::default());
    };

    let api = WebApi::new();

    match auth::restore_login(&stored, &data_dir).await {
        Ok(toks) => establish(&app, &state, &api, toks).await,
        Err(e) => {
            // Only discard the stored tokens when Spotify says the grant
            // itself is dead. A network blip at startup, a 429, or a 5xx are
            // all transient — deleting on those was silently logging the user
            // out roughly whenever the app started faster than the network
            // came up.
            if auth::is_grant_rejected(&e) {
                log::warn!("stored refresh token rejected by Spotify, clearing it: {e}");
                auth::clear_stored_tokens(&data_dir);
            } else {
                log::warn!("could not restore session (tokens kept, will retry next launch): {e}");
            }
            Ok(AuthState::default())
        }
    }
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    state.device_auth.cancel().await;
    state.sleep_timer.cancel(None).await;
    state.telemetry.finish_all("logout").await;
    let episode_checkpoint = {
        let mut playback = state.playback.write().await;
        playback.refresh_position();
        playback
            .track
            .as_ref()
            .filter(|track| track.uri.starts_with("spotify:episode:"))
            .map(|track| (track.uri.clone(), u64::from(playback.position_ms)))
    };
    if let Some(s) = state.spotify.write().await.take() {
        if let Some((uri, position_ms)) = episode_checkpoint {
            crate::podcasts::spawn_report(s.session.clone(), uri, position_ms, false);
        }
        let _ = s.spirc.shutdown();
        s.refresh_task.abort();
        s.remote_task.abort();
        s.connect_state_task.abort();
        if let Some(task) = s.friends_task {
            task.abort();
        }
    }
    // Drop the jam controller too: its dealer listener and event forwarder
    // must not outlive the session they authenticate against.
    state.jams.write().await.take();
    state.tokens.set(String::new()).await;
    *state.auth.write().await = AuthState::default();
    *state.playback.write().await = PlaybackState::default();
    *state.queue.write().await = QueueView::default();
    state.lyrics_cache.write().await.clear();
    *state.friend_activity.write().await = crate::friends::FriendFeed::default();

    if let Ok(dir) = app.path().app_data_dir() {
        auth::clear_stored_tokens(&dir);
    }

    let _ = app.emit(events::AUTH, AuthState::default());
    Ok(())
}

// ---- playback -----------------------------------------------------------

#[tauri::command]
pub async fn get_playback(state: State<'_, AppState>) -> AppResult<PlaybackState> {
    Ok(player::snapshot(&state).await)
}

#[tauri::command]
pub async fn play(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.play()).await
    } else {
        remote_put(&state, "/me/player/play", &[]).await
    }
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.pause()).await
    } else {
        remote_put(&state, "/me/player/pause", &[]).await
    }
}

#[tauri::command]
pub async fn play_pause(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.play_pause()).await
    } else if state.playback.read().await.is_playing {
        remote_put(&state, "/me/player/pause", &[]).await
    } else {
        remote_put(&state, "/me/player/play", &[]).await
    }
}

#[tauri::command]
pub async fn next_track(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.next()).await
    } else {
        remote_post(&state, "/me/player/next", &[]).await
    }
}

#[tauri::command]
pub async fn previous_track(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.prev()).await
    } else {
        remote_post(&state, "/me/player/previous", &[]).await
    }
}

#[tauri::command]
pub async fn seek(app: AppHandle, state: State<'_, AppState>, position_ms: u32) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        let was_playing = state.playback.read().await.is_playing;
        let crossfade_enabled = app
            .path()
            .app_data_dir()
            .ok()
            .map(|dir| auth::settings_or_default(&dir).crossfade_seconds > 0)
            .unwrap_or(false);

        // The reviewed librespot PR clears an in-flight fade on Pause but not
        // on Seek. Bracket an explicit local seek so an outgoing decoder can
        // never continue underneath audio from the new position. Preserve a
        // user-paused state by only resuming when playback was running.
        if crossfade_enabled && was_playing {
            with_spirc(&state, |s| s.pause()).await?;
            let seek_result = with_spirc(&state, |s| s.set_position_ms(position_ms)).await;
            let resume_result = with_spirc(&state, |s| s.play()).await;
            seek_result.and(resume_result)
        } else {
            with_spirc(&state, |s| s.set_position_ms(position_ms)).await
        }
    } else {
        remote_put(
            &state,
            "/me/player/seek",
            &[("position_ms", position_ms.to_string())],
        )
        .await
    }
}

/// `percent` is 0..=100; librespot's own scale is 0..=65535.
#[tauri::command]
pub async fn set_volume(state: State<'_, AppState>, percent: u8) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        let volume = player::percent_to_volume(percent);
        with_spirc(&state, |s| s.set_volume(volume)).await
    } else {
        remote_put(
            &state,
            "/me/player/volume",
            &[("volume_percent", percent.min(100).to_string())],
        )
        .await
    }
}

#[tauri::command]
pub async fn set_shuffle(state: State<'_, AppState>, shuffle: bool) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.shuffle(shuffle)).await
    } else {
        remote_put(
            &state,
            "/me/player/shuffle",
            &[("state", shuffle.to_string())],
        )
        .await
    }
}

#[tauri::command]
pub async fn set_repeat(state: State<'_, AppState>, context: bool, track: bool) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.repeat(context)).await?;
        with_spirc(&state, |s| s.repeat_track(track)).await
    } else {
        let repeat = if track {
            "track"
        } else if context {
            "context"
        } else {
            "off"
        };
        remote_put(
            &state,
            "/me/player/repeat",
            &[("state", repeat.to_string())],
        )
        .await
    }
}

#[tauri::command]
pub async fn get_sleep_timer(
    state: State<'_, AppState>,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    Ok(state.sleep_timer.status().await)
}

#[tauri::command]
pub async fn start_sleep_timer(
    app: AppHandle,
    state: State<'_, AppState>,
    seconds: u64,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    state.sleep_timer.start_duration(app, seconds).await
}

#[tauri::command]
pub async fn sleep_at_end_of_track(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    Ok(state.sleep_timer.start_end_of_track(&app).await)
}

#[tauri::command]
pub async fn cancel_sleep_timer(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    Ok(state.sleep_timer.cancel(Some(&app)).await)
}

/// Loads a *context* (playlist, album, artist, or the liked-songs collection)
/// and optionally starts at a specific track within it.
///
/// This is the path that matters for playback feeling correct: loading a bare
/// track URI plays that one track and stops, whereas loading the containing
/// context lets playback continue through the rest of it.
#[tauri::command]
pub async fn load_context(
    state: State<'_, AppState>,
    context_uri: String,
    track_uri: Option<String>,
) -> AppResult<()> {
    // Taking over playback is the point of an explicit load, so activate here
    // rather than at login — see the note in `player::start_session`. Skipped
    // when already active, which librespot would otherwise log as
    // "SpircCommand::Activate will be ignored while already active".
    let activate = !state.playback.read().await.is_active_device;

    let requested_episode = track_uri
        .as_deref()
        .filter(|uri| uri.starts_with("spotify:episode:"))
        .or_else(|| {
            context_uri
                .starts_with("spotify:episode:")
                .then_some(context_uri.as_str())
        });
    let seek_to = if let Some(uri) = requested_episode {
        let session = state
            .spotify
            .read()
            .await
            .as_ref()
            .map(|spotify| spotify.session.clone())
            .ok_or(AppError::NotLoggedIn)?;
        match crate::podcasts::get(&session, uri).await {
            Ok(resume) if !resume.completed => resume.position_ms.min(u64::from(u32::MAX)) as u32,
            Ok(_) => 0,
            Err(error) => {
                log::debug!(target: "spotify.podcasts", "episode resume lookup failed safely: {error}");
                0
            }
        }
    } else {
        0
    };

    with_spirc(&state, move |s| {
        if activate {
            s.activate()?;
        }
        s.load(LoadRequest::from_context_uri(
            context_uri,
            LoadRequestOptions {
                start_playing: true,
                seek_to,
                playing_track: track_uri.map(PlayingTrack::Uri),
                ..Default::default()
            },
        ))
    })
    .await
}

/// Loads an explicit list of track URIs as an ad-hoc context.
///
/// Used where there is no container to play from — search results, or picking
/// an entry out of the queue.
#[tauri::command]
pub async fn load_tracks(
    state: State<'_, AppState>,
    uris: Vec<String>,
    start_uri: Option<String>,
) -> AppResult<()> {
    if uris.is_empty() {
        return Ok(());
    }
    let activate = !state.playback.read().await.is_active_device;

    let requested_episode = start_uri
        .as_deref()
        .or_else(|| uris.first().map(String::as_str))
        .filter(|uri| uri.starts_with("spotify:episode:"));
    let seek_to = if let Some(uri) = requested_episode {
        let session = state
            .spotify
            .read()
            .await
            .as_ref()
            .map(|spotify| spotify.session.clone())
            .ok_or(AppError::NotLoggedIn)?;
        match crate::podcasts::get(&session, uri).await {
            Ok(resume) if !resume.completed => resume.position_ms.min(u64::from(u32::MAX)) as u32,
            Ok(_) => 0,
            Err(error) => {
                log::debug!(target: "spotify.podcasts", "episode resume lookup failed safely: {error}");
                0
            }
        }
    } else {
        0
    };

    with_spirc(&state, move |s| {
        if activate {
            s.activate()?;
        }
        s.load(LoadRequest::from_tracks(
            uris,
            LoadRequestOptions {
                start_playing: true,
                seek_to,
                playing_track: start_uri.map(PlayingTrack::Uri),
                ..Default::default()
            },
        ))
    })
    .await
}

// ---- connect ------------------------------------------------------------

#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> AppResult<Vec<Device>> {
    let t = token(&state).await?;
    connect::list_devices(&WebApi::new(), &t).await
}

#[tauri::command]
pub async fn transfer_playback(
    state: State<'_, AppState>,
    device_id: String,
    play: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    connect::transfer_playback(&WebApi::new(), &t, &device_id, play).await
}

/// Pulls playback back to this app by activating our own Spirc device.
#[tauri::command]
pub async fn activate_this_device(state: State<'_, AppState>) -> AppResult<()> {
    with_spirc(&state, |s| s.activate()).await
}

// ---- library ------------------------------------------------------------

#[tauri::command]
pub async fn get_playlists(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<PlaylistSummary>> {
    let t = token(&state).await?;
    library::playlists(&WebApi::new(), &t, limit.unwrap_or(50), offset.unwrap_or(0)).await
}

#[tauri::command]
pub async fn get_playlist_tracks(
    state: State<'_, AppState>,
    playlist_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::playlist_tracks(
        &WebApi::new(),
        &t,
        &playlist_id,
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
    .await
}

#[tauri::command]
pub async fn get_saved_tracks(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::saved_tracks(&WebApi::new(), &t, limit.unwrap_or(50), offset.unwrap_or(0)).await
}

#[tauri::command]
pub async fn get_saved_albums(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<AlbumSummary>> {
    let t = token(&state).await?;
    library::saved_albums(&WebApi::new(), &t, limit.unwrap_or(50), offset.unwrap_or(0)).await
}

/// `after` is the `next` cursor from the previous page, not an item count —
/// `/me/following` is cursor-paginated. Omit it for the first page.
#[tauri::command]
pub async fn get_followed_artists(
    state: State<'_, AppState>,
    limit: Option<u32>,
    after: Option<String>,
) -> AppResult<ArtistPage> {
    let t = token(&state).await?;
    library::followed_artists(&WebApi::new(), &t, limit.unwrap_or(50), after.as_deref()).await
}

#[tauri::command]
pub async fn get_followed_releases(
    state: State<'_, AppState>,
    after: Option<String>,
) -> AppResult<FollowedReleasePage> {
    let t = token(&state).await?;
    library::followed_releases(&WebApi::new(), &t, after.as_deref()).await
}

#[tauri::command]
pub async fn get_recently_played(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<RecentActivityItem>> {
    let t = token(&state).await?;
    library::recently_played(&WebApi::new(), &t, limit.unwrap_or(50)).await
}

/// Combines Spotify's real recent-play window with account-scoped local open
/// and play history. This intentionally does not claim to reproduce Spotify's
/// private Home ranking.
#[tauri::command]
pub async fn get_quick_access(
    app: AppHandle,
    state: State<'_, AppState>,
    recent: Vec<RecentActivityItem>,
    limit: Option<usize>,
) -> AppResult<Vec<RecentActivityItem>> {
    let account = state
        .auth
        .read()
        .await
        .user_id
        .clone()
        .ok_or(AppError::NotLoggedIn)?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(e.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || {
        Ok(crate::relevance::rank(
            &data_dir,
            &account,
            recent,
            limit.unwrap_or(6).min(12),
        ))
    })
    .await
    .map_err(|e| AppError::Other(format!("relevance ranking failed: {e}")))?
}

#[tauri::command]
pub async fn record_relevance(
    app: AppHandle,
    state: State<'_, AppState>,
    item: RecentActivityItem,
    played: bool,
) -> AppResult<()> {
    let account = state
        .auth
        .read()
        .await
        .user_id
        .clone()
        .ok_or(AppError::NotLoggedIn)?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(e.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::relevance::record(&data_dir, &account, item, played)
    })
    .await
    .map_err(|e| AppError::Other(format!("could not persist relevance history: {e}")))?
}

#[tauri::command]
pub async fn get_album_tracks(
    state: State<'_, AppState>,
    album_id: String,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::album_tracks(&WebApi::new(), &t, &album_id).await
}

#[tauri::command]
pub async fn set_tracks_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
    saved: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::set_tracks_saved(&WebApi::new(), &t, &ids, saved).await
}

#[tauri::command]
pub async fn set_albums_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
    saved: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::set_albums_saved(&WebApi::new(), &t, &ids, saved).await
}

#[tauri::command]
pub async fn set_artists_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
    saved: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::set_artists_saved(&WebApi::new(), &t, &ids, saved).await
}

#[tauri::command]
pub async fn get_tracks_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<bool>> {
    let t = token(&state).await?;
    library::tracks_saved(&WebApi::new(), &t, &ids).await
}

#[tauri::command]
pub async fn get_albums_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<bool>> {
    let t = token(&state).await?;
    library::albums_saved(&WebApi::new(), &t, &ids).await
}

#[tauri::command]
pub async fn get_artists_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<bool>> {
    let t = token(&state).await?;
    library::artists_saved(&WebApi::new(), &t, &ids).await
}

#[tauri::command]
pub async fn get_liked_tracks_by_artist(
    state: State<'_, AppState>,
    artist_id: String,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::liked_tracks_by_artist(&WebApi::new(), &t, &artist_id).await
}

#[tauri::command]
pub async fn get_artist_top_tracks(
    state: State<'_, AppState>,
    artist_id: String,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::artist_tracks(&WebApi::new(), &t, &artist_id).await
}

#[tauri::command]
pub async fn get_artist_albums(
    state: State<'_, AppState>,
    artist_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<AlbumPage> {
    let t = token(&state).await?;
    library::artist_albums(
        &WebApi::new(),
        &t,
        &artist_id,
        limit.unwrap_or(10),
        offset.unwrap_or(0),
    )
    .await
}

#[tauri::command]
pub async fn get_artist(state: State<'_, AppState>, artist_id: String) -> AppResult<ArtistSummary> {
    let t = token(&state).await?;
    library::artist(&WebApi::new(), &t, &artist_id).await
}

#[tauri::command]
pub async fn get_artist_concerts(
    state: State<'_, AppState>,
    artist_id: String,
    locale: Option<String>,
) -> AppResult<crate::spotify::ConcertFeed> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    state
        .internal_spotify
        .artist_concerts(&session, &artist_id, locale.as_deref().unwrap_or(""))
        .await
}

#[tauri::command]
pub async fn get_track_credits(
    state: State<'_, AppState>,
    track_uri: String,
) -> AppResult<crate::spotify::TrackCredits> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    state
        .internal_spotify
        .track_credits(&session, &track_uri)
        .await
}

#[tauri::command]
pub async fn get_episode_resume(
    state: State<'_, AppState>,
    episode_uri: String,
) -> AppResult<crate::podcasts::EpisodeResume> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    crate::podcasts::get(&session, &episode_uri).await
}

#[tauri::command]
pub async fn set_episode_completed(
    state: State<'_, AppState>,
    episode_uri: String,
    completed: bool,
) -> AppResult<()> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    crate::podcasts::set_completed(&session, &episode_uri, completed).await
}

#[tauri::command]
pub async fn get_telemetry_status(
    state: State<'_, AppState>,
) -> AppResult<crate::telemetry::TelemetryStatus> {
    Ok(state.telemetry.status().await)
}

#[tauri::command]
pub async fn get_top_tracks(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::top_tracks(&WebApi::new(), &t, limit.unwrap_or(20)).await
}

#[tauri::command]
pub async fn get_top_artists(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<ArtistSummary>> {
    let t = token(&state).await?;
    library::top_artists(&WebApi::new(), &t, limit.unwrap_or(10)).await
}

#[tauri::command]
pub async fn get_lyrics(
    state: State<'_, AppState>,
    track_uri: String,
    track_name: String,
    artist_name: String,
    album_name: String,
    duration_ms: u32,
) -> AppResult<LyricsResult> {
    if let Some(cached) = state.lyrics_cache.read().await.get(&track_uri).cloned() {
        return Ok(cached);
    }

    // Lyrics are a separate, read-only integration. Requiring an active
    // session prevents stale track details from making background requests
    // after logout.
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    let result = crate::lyrics::fetch(
        &session,
        &track_uri,
        &track_name,
        &artist_name,
        &album_name,
        duration_ms,
    )
    .await?;

    let mut cache = state.lyrics_cache.write().await;
    const MAX_LYRICS_CACHE_ENTRIES: usize = 64;
    if cache.len() >= MAX_LYRICS_CACHE_ENTRIES && !cache.contains_key(&track_uri) {
        // Lyrics are immutable enough for a session; clearing at the hard
        // bound is cheap and keeps memory use deterministic without another
        // cache dependency.
        cache.clear();
    }
    cache.insert(track_uri, result.clone());
    Ok(result)
}

#[tauri::command]
pub async fn get_friend_activity(
    state: State<'_, AppState>,
) -> AppResult<crate::friends::FriendFeed> {
    if state.spotify.read().await.is_none() {
        return Err(AppError::NotLoggedIn);
    }
    Ok(state.friend_activity.read().await.clone())
}

#[tauri::command]
pub async fn get_user_profile(
    state: State<'_, AppState>,
    username: Option<String>,
) -> AppResult<crate::profiles::UserProfile> {
    let session = {
        let spotify = state.spotify.read().await;
        spotify
            .as_ref()
            .map(|spotify| spotify.session.clone())
            .ok_or(AppError::NotLoggedIn)?
    };
    let authenticated_username = state.auth.read().await.user_id.clone();
    let username = match username.filter(|value| !value.trim().is_empty()) {
        Some(username) => username,
        None => authenticated_username
            .clone()
            .ok_or_else(|| AppError::Auth("Spotify session has no username".to_string()))?,
    };
    let mut profile = crate::profiles::fetch(&session, &username).await?;
    // Older responses omit `is_current_user`. The authenticated username is
    // authoritative and prevents the UI treating the owner's profile as a
    // public visitor view when that optional response field is absent.
    if authenticated_username.is_some_and(|current| current.eq_ignore_ascii_case(&profile.username))
    {
        profile.is_current_user = true;
    }
    Ok(profile)
}

// ---- search -------------------------------------------------------------

#[tauri::command]
pub async fn search_spotify(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<SearchResults> {
    let t = token(&state).await?;
    let limit = limit.unwrap_or(search::MAX_SEARCH_LIMIT);
    search::search(&WebApi::new(), &t, &query, limit, offset.unwrap_or(0)).await
}

// ---- queue --------------------------------------------------------------

#[tauri::command]
pub async fn get_queue(state: State<'_, AppState>) -> AppResult<QueueView> {
    let t = token(&state).await?;
    let web = queue::get_queue(&WebApi::new(), &t).await?;
    let merged = queue::merge_metadata(&*state.queue.read().await, web);
    *state.queue.write().await = merged.clone();
    Ok(merged)
}

#[tauri::command]
pub async fn add_to_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    uri: String,
) -> AppResult<()> {
    let t = token(&state).await?;
    let api = WebApi::new();
    queue::add_to_queue(&api, &t, &uri).await?;
    if let Ok(web) = queue::get_queue(&api, &t).await {
        let merged = queue::merge_metadata(&*state.queue.read().await, web);
        *state.queue.write().await = merged.clone();
        let _ = app.emit(events::QUEUE, merged);
    }
    Ok(())
}

// ---- jams (experimental) --------------------------------------------------

/// Returns the controller, building it lazily. Requires a live session (the
/// jams module needs the user's bearer token), but the check also keeps a
/// logged-out app from spawning a dealer listener that would just reconnect
/// forever with no token.
async fn ensure_jams(app: &AppHandle, state: &AppState) -> AppResult<Arc<JamController>> {
    if let Some(ctrl) = state.jams.read().await.as_ref() {
        return Ok(ctrl.clone());
    }
    // Jams hang off the librespot session (device id, client-token, dealer
    // connection id), so the controller cannot be built before it exists.
    let session = {
        let guard = state.spotify.read().await;
        let Some(session) = guard.as_ref() else {
            return Err(AppError::NotLoggedIn);
        };
        session.session.clone()
    };
    let ctrl = Arc::new(JamController::build(app, session, &state.tokens).await?);
    let mut guard = state.jams.write().await;
    if let Some(existing) = guard.as_ref() {
        return Ok(existing.clone());
    }
    guard.replace(ctrl.clone());
    Ok(ctrl)
}

/// Configuration + current session, so the Jams view can explain what still
/// needs capturing (endpoints/hashes) without firing a request.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JamStatus {
    pub spclient_endpoints: usize,
    pub pathfinder_hashes: usize,
    pub session: Option<JamSession>,
}

#[tauri::command]
pub async fn get_jam_status(app: AppHandle, state: State<'_, AppState>) -> AppResult<JamStatus> {
    // Not logged in is not an error here: the view only renders while logged
    // in, but a reload can race that, and a status read must not spawn a
    // dealer loop on its own.
    let Ok(ctrl) = ensure_jams(&app, &state).await else {
        return Ok(JamStatus {
            spclient_endpoints: 0,
            pathfinder_hashes: 0,
            session: None,
        });
    };
    let config = ctrl.config();
    Ok(JamStatus {
        spclient_endpoints: config.spclient_endpoints.len(),
        pathfinder_hashes: config.pathfinder_hashes.len(),
        session: ctrl.session().await,
    })
}

#[tauri::command]
pub async fn create_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.create(None).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn refresh_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.refresh_session().await.map_err(AppError::from)
}

#[tauri::command]
pub async fn join_jam(
    app: AppHandle,
    state: State<'_, AppState>,
    jam_id: String,
) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.join(&jam_id).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn leave_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.leave().await.map_err(AppError::from)
}

/// Adds the currently playing track to the active jam.
#[tauri::command]
pub async fn add_track_to_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    let Some(track) = state.playback.read().await.track.clone() else {
        return Err(AppError::Playback("Nothing is playing.".to_string()));
    };
    ctrl.add_track(&track.uri).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn set_jam_queue_control(
    app: AppHandle,
    state: State<'_, AppState>,
    allowed: bool,
) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.set_queue_control(allowed)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn kick_jam_member(
    app: AppHandle,
    state: State<'_, AppState>,
    member_id: String,
) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.kick(&member_id).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn end_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.end().await.map_err(AppError::from)
}

#[cfg(test)]
mod settings_validation_tests {
    use super::*;

    #[test]
    fn crossfade_matches_spotify_supported_range() {
        assert!(validate_crossfade(0).is_ok());
        assert!(validate_crossfade(12).is_ok());
        assert!(validate_crossfade(13).is_err());
    }
}
