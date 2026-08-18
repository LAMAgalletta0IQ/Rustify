use librespot::connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
use librespot::core::authentication::Credentials;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::audio::{
    self, AudioDevice, AudioStatus, EqualizerPreset, EqualizerSettings, StreamQuality,
};
use crate::auth;
use crate::connect::{self, Device};
use crate::error::{AppError, AppResult};
use crate::library::{
    self, AlbumPage, AlbumSummary, ArtistPage, FollowedReleasePage, PlaylistSummary,
    RecentActivityItem, TrackSummary,
};
use crate::lyrics::LyricsResult;
use crate::player;
use crate::queue::{self, QueueView};
use crate::search::ArtistSummary;
use crate::search::{self, SearchResults};
use crate::state::{events, AppState, AuthState, PlaybackState, SpotifySession};
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
    pub output_device: Option<String>,
    pub equalizer: EqualizerSettings,
}

impl From<auth::Settings> for AppSettings {
    fn from(value: auth::Settings) -> Self {
        Self {
            default_volume_percent: value.default_volume_percent,
            reduce_motion: value.reduce_motion,
            cache_limit_mb: value.cache_limit_mb,
            audio_quality: value.audio_quality,
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
    audio::validate_equalizer(&settings.equalizer)?;
    audio::validate_output_device(settings.output_device.as_deref())?;

    let mut persisted = auth::settings_or_default(&data_dir);
    persisted.default_volume_percent = settings.default_volume_percent;
    persisted.reduce_motion = settings.reduce_motion;
    persisted.cache_limit_mb = settings.cache_limit_mb;
    persisted.audio_quality = settings.audio_quality;
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
    }

    // Reflects playback on the user's other devices, so opening the app while
    // listening on a phone shows the current track instead of "Nothing playing".
    let remote_task = player::spawn_remote_poller(app.clone(), state.tokens.clone());

    *state.spotify.write().await = Some(SpotifySession {
        session: started.session,
        spirc: started.spirc,
        refresh_task,
        remote_task,
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

    // No point attempting a restore that cannot complete the Web API half —
    // the UI shows the Setup screen instead until a client ID is saved.
    if auth::webapi_client_id(&data_dir).is_err() {
        return Ok(AuthState::default());
    }

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
    if let Some(s) = state.spotify.write().await.take() {
        let _ = s.spirc.shutdown();
        s.refresh_task.abort();
        s.remote_task.abort();
    }
    state.tokens.set(String::new()).await;
    *state.auth.write().await = AuthState::default();
    *state.playback.write().await = PlaybackState::default();

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
    with_spirc(&state, |s| s.play()).await
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>) -> AppResult<()> {
    with_spirc(&state, |s| s.pause()).await
}

#[tauri::command]
pub async fn play_pause(state: State<'_, AppState>) -> AppResult<()> {
    with_spirc(&state, |s| s.play_pause()).await
}

#[tauri::command]
pub async fn next_track(state: State<'_, AppState>) -> AppResult<()> {
    with_spirc(&state, |s| s.next()).await
}

#[tauri::command]
pub async fn previous_track(state: State<'_, AppState>) -> AppResult<()> {
    with_spirc(&state, |s| s.prev()).await
}

#[tauri::command]
pub async fn seek(state: State<'_, AppState>, position_ms: u32) -> AppResult<()> {
    with_spirc(&state, |s| s.set_position_ms(position_ms)).await
}

/// `percent` is 0..=100; librespot's own scale is 0..=65535.
#[tauri::command]
pub async fn set_volume(state: State<'_, AppState>, percent: u8) -> AppResult<()> {
    let v = player::percent_to_volume(percent);
    with_spirc(&state, |s| s.set_volume(v)).await
}

#[tauri::command]
pub async fn set_shuffle(state: State<'_, AppState>, shuffle: bool) -> AppResult<()> {
    with_spirc(&state, |s| s.shuffle(shuffle)).await
}

#[tauri::command]
pub async fn set_repeat(state: State<'_, AppState>, context: bool, track: bool) -> AppResult<()> {
    with_spirc(&state, |s| s.repeat(context)).await?;
    with_spirc(&state, |s| s.repeat_track(track)).await
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

    with_spirc(&state, move |s| {
        if activate {
            s.activate()?;
        }
        s.load(LoadRequest::from_context_uri(
            context_uri,
            LoadRequestOptions {
                start_playing: true,
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

    with_spirc(&state, move |s| {
        if activate {
            s.activate()?;
        }
        s.load(LoadRequest::from_tracks(
            uris,
            LoadRequestOptions {
                start_playing: true,
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
    track_name: String,
    artist_name: String,
    album_name: String,
    duration_ms: u32,
) -> AppResult<LyricsResult> {
    // Lyrics are a separate, read-only integration. Requiring an active
    // session prevents stale track details from making background requests
    // after logout.
    if state.spotify.read().await.is_none() {
        return Err(AppError::NotLoggedIn);
    }
    crate::lyrics::fetch(&track_name, &artist_name, &album_name, duration_ms).await
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
    queue::get_queue(&WebApi::new(), &t).await
}

#[tauri::command]
pub async fn add_to_queue(state: State<'_, AppState>, uri: String) -> AppResult<()> {
    let t = token(&state).await?;
    queue::add_to_queue(&WebApi::new(), &t, &uri).await
}
