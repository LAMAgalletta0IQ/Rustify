//! Auth/session lifecycle: login, logout, restore, device authorization,
//! settings and audio-device configuration.

use librespot::core::authentication::Credentials;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::audio::{
    self, AudioDevice, AudioStatus, EqualizerPreset, EqualizerSettings, LoudnessSettings,
    StreamQuality,
};
use crate::auth;
use crate::error::{AppError, AppResult};
use crate::player;
use crate::queue::QueueView;
use crate::state::{events, AppState, AuthState, ConnectionStatus, PlaybackState, SpotifySession};
use crate::webapi::WebApi;

use super::device_name;

#[tauri::command]
pub async fn get_auth_state(state: State<'_, AppState>) -> AppResult<AuthState> {
    Ok(state.auth.read().await.clone())
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
    /// The configured ID itself, so Settings can show and edit what is in use
    /// rather than only whether *something* is. Safe to hand to the webview: a
    /// Spotify Client ID is not a secret — it travels in the clear in every
    /// OAuth redirect and this flow is PKCE with no client secret.
    pub client_id: Option<String>,
    /// True when the ID came from `RUSTIFY_CLIENT_ID` rather than
    /// `settings.json`. The env var wins in `auth::webapi_client_id`, so
    /// editing the saved value would have no visible effect while it is set —
    /// Settings disables the field and says so instead of silently no-opping.
    pub client_id_from_env: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub reduce_motion: bool,
    pub cache_limit_mb: u32,
    pub audio_quality: StreamQuality,
    pub crossfade_seconds: u8,
    pub output_device: Option<String>,
    pub equalizer: EqualizerSettings,
    pub loudness: LoudnessSettings,
    pub friends_panel_open: bool,
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
            reduce_motion: value.reduce_motion,
            cache_limit_mb: value.cache_limit_mb,
            audio_quality: value.audio_quality,
            crossfade_seconds: value.crossfade_seconds,
            output_device: value.output_device,
            equalizer: value.equalizer,
            loudness: value.loudness,
            friends_panel_open: value.friends_panel_open,
        }
    }
}

#[tauri::command]
pub fn get_login_info(app: AppHandle) -> AppResult<LoginInfo> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let client_id = auth::webapi_client_id(&data_dir).ok();
    let client_id_from_env = std::env::var(auth::CLIENT_ID_ENV)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    Ok(LoginInfo {
        private_client_id: client_id.is_some(),
        client_id_env: auth::CLIENT_ID_ENV,
        webapi_redirect_uri: auth::webapi_redirect_uri(),
        client_id,
        client_id_from_env,
    })
}

/// Forgets the saved Web API client ID, sending the app back to the first-run
/// Setup screen on the next launch.
///
/// Deliberately does *not* sign out on its own — the caller decides. Settings
/// pairs it with `logout` because the stored OAuth grant belongs to the app
/// being cleared, but "clear the ID" and "end the session" are separate
/// actions and conflating them here would make the command untestable in one
/// direction.
#[tauri::command]
pub fn clear_client_id(app: AppHandle) -> AppResult<()> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let mut settings = auth::settings_or_default(&data_dir);
    settings.webapi_client_id = None;
    auth::save_settings(&data_dir, &settings)
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

    if !(128..=8192).contains(&settings.cache_limit_mb) {
        return Err(AppError::BadRequest(
            "Cache size must be between 128 MB and 8192 MB.".into(),
        ));
    }
    validate_crossfade(settings.crossfade_seconds)?;
    audio::validate_equalizer(&settings.equalizer)?;
    audio::validate_loudness(&settings.loudness)?;

    let mut persisted = auth::settings_or_default(&data_dir);
    persisted.reduce_motion = settings.reduce_motion;
    persisted.cache_limit_mb = settings.cache_limit_mb;
    persisted.audio_quality = settings.audio_quality;
    persisted.crossfade_seconds = settings.crossfade_seconds;
    persisted.output_device = settings.output_device;
    persisted.equalizer = settings.equalizer;
    persisted.loudness = settings.loudness;
    persisted.friends_panel_open = settings.friends_panel_open;
    auth::save_settings(&data_dir, &persisted)?;
    state
        .audio
        .configure(persisted.output_device.clone(), persisted.equalizer.clone());
    Ok(persisted.into())
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioConfiguration {
    pub output_device: Option<String>,
    pub equalizer: EqualizerSettings,
}

/// Applies sound changes to the active sink without touching disk. Slider
/// input uses this path so the graph, controls, and audio thread never drift
/// apart while the debounced persistence call is still pending.
#[tauri::command]
pub fn configure_audio(
    state: State<'_, AppState>,
    configuration: AudioConfiguration,
) -> AppResult<AudioStatus> {
    audio::validate_equalizer(&configuration.equalizer)?;
    state
        .audio
        .configure(configuration.output_device, configuration.equalizer);
    Ok(state.audio.status())
}

/// Persists only output/EQ settings, leaving unrelated Settings drafts alone.
/// A saved device may be disconnected: the sink falls back to the system
/// default and retries it after hot-plugging instead of rejecting the setting.
#[tauri::command]
pub fn update_audio_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    configuration: AudioConfiguration,
) -> AppResult<AppSettings> {
    audio::validate_equalizer(&configuration.equalizer)?;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;
    let mut persisted = auth::settings_or_default(&data_dir);
    persisted.output_device = configuration.output_device;
    persisted.equalizer = configuration.equalizer;
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
    // Claimed before anything is built, so the pump this session is about to
    // spawn carries an id no earlier pump can match. See
    // `AppState::session_generation` — this is what lets the playback watchdog
    // tell a crash apart from a deliberate replacement.
    let generation = state.next_session_generation();

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
    audio::validate_loudness(&settings.loudness)?;
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
            initial_volume_percent: settings.last_volume_percent,
            cache_limit_mb: settings.cache_limit_mb,
            quality: settings.audio_quality,
            crossfade_seconds: settings.crossfade_seconds,
            loudness: settings.loudness,
        },
        generation,
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
            log::debug!(target: "spotify.social", "friend activity subscription failed: {error}");
            // A failure to register the Dealer subscription is a connectivity
            // problem, not evidence Spotify refused the capability (that only
            // comes from a 403/404 on the actual presence request, which
            // hasn't happened yet at this point) — Failed, not Unavailable.
            *state.friend_activity.write().await = crate::friends::FriendFeed {
                status: crate::friends::FriendFeedStatus::Failed,
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

    let toks = auth::interactive_login(&data_dir).await?;
    establish(&app, &state, &state.web_api, toks).await
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
    let tokens = auth::complete_device_authorization(&state.device_auth).await?;
    establish(&app, &state, &state.web_api, tokens).await
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

    match auth::restore_login(&stored, &data_dir).await {
        Ok(toks) => establish(&app, &state, &state.web_api, toks).await,
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

/// Rebuilds the librespot session in place, for the playback watchdog in
/// `player::recover_closed_session`.
///
/// Not a command: nothing in the webview asks for this. It reuses the stored
/// refresh tokens exactly as `restore_session` does, but never clears them and
/// never returns a logged-out `AuthState` — a failure here means "could not
/// reconnect right now", and the caller retries. Deleting tokens on a
/// reconnect failure would turn a dropped socket into a forced re-login, which
/// is the outcome this whole path exists to avoid.
pub(crate) async fn rebuild_session(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let stored = auth::load_stored_tokens(&data_dir).ok_or(AppError::NotLoggedIn)?;
    let toks = auth::restore_login(&stored, &data_dir).await?;
    establish(app, &state, &state.web_api, toks)
        .await
        .map(|_| ())
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    // Retire the generation first. Shutting the Spirc down below closes the
    // player event channel, and the watchdog on the other end of it must read
    // that as "deliberate" rather than as a crash to recover from.
    state.next_session_generation();
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
    // must not outlive the session they authenticate against. `Drop` on
    // `JamController` explicitly aborts the forward task; without that the
    // task would only be detached, not cancelled, and would keep emitting
    // `jams:changed` after this logout.
    state.jams.write().await.take();
    state.tokens.set(String::new()).await;
    *state.auth.write().await = AuthState::default();
    *state.playback.write().await = PlaybackState::default();
    *state.queue.write().await = QueueView::default();
    state.lyrics_cache.write().await.clear();
    state.lyrics_requests.lock().await.clear();
    state.audio_capability_cache.write().await.clear();
    *state.friend_activity.write().await = crate::friends::FriendFeed::default();

    if let Ok(dir) = app.path().app_data_dir() {
        auth::clear_stored_tokens(&dir);
    }

    let _ = app.emit(events::AUTH, AuthState::default());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfade_matches_spotify_supported_range() {
        assert!(validate_crossfade(0).is_ok());
        assert!(validate_crossfade(12).is_ok());
        assert!(validate_crossfade(13).is_err());
    }
}
