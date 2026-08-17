use librespot::connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
use librespot::core::authentication::Credentials;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth;
use crate::connect::{self, Device};
use crate::error::{AppError, AppResult};
use crate::library::{self, AlbumSummary, ArtistPage, PlaylistSummary, TrackSummary};
use crate::player;
use crate::queue::{self, QueueView};
use crate::search::{self, SearchResults};
use crate::state::{events, AppState, AuthState, PlaybackState, SpotifySession};
use crate::webapi::WebApi;

/// Pulls the Web API bearer token out of the live session, or fails cleanly if
/// the user is not logged in. Every Web API command starts here.
async fn token(state: &AppState) -> AppResult<String> {
    if state.spotify.read().await.is_none() {
        return Err(AppError::NotLoggedIn);
    }
    Ok(state.tokens.get().await)
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
/// guessing. Read at render time, not cached, because `.env` is only loaded at
/// startup but the value is cheap to recompute.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInfo {
    /// True when a private Web API client ID is configured, which means the
    /// login opens the browser twice and rate limits are far less likely.
    pub private_client_id: bool,
    /// Name of the variable to set, so the UI never hardcodes it.
    pub client_id_env: &'static str,
    /// Redirect URI the user must register against their own app.
    pub webapi_redirect_uri: String,
}

#[tauri::command]
pub fn get_login_info() -> LoginInfo {
    LoginInfo {
        private_client_id: auth::webapi_client_id().is_some(),
        client_id_env: auth::CLIENT_ID_ENV,
        webapi_redirect_uri: auth::webapi_redirect_uri(),
    }
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
    auth::save_stored_tokens(&data_dir, &stored)?;

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
    )
    .await?;

    // Access tokens last ~1h; without this every Web API call would start
    // failing mid-session.
    let refresh_task = auth::spawn_refresher(
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
    let api = WebApi::new();
    let toks = auth::interactive_login().await?;
    establish(&app, &state, &api, toks).await
}

/// Attempted once at startup. Returns a logged-out state rather than an error
/// when there is nothing stored, so the UI can just show the login screen.
#[tauri::command]
pub async fn restore_session(app: AppHandle, state: State<'_, AppState>) -> AppResult<AuthState> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let Some(stored) = auth::load_stored_tokens(&data_dir) else {
        return Ok(AuthState::default());
    };

    let api = WebApi::new();

    match auth::restore_login(&stored).await {
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
pub async fn set_repeat(
    state: State<'_, AppState>,
    context: bool,
    track: bool,
) -> AppResult<()> {
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

/// TEMPORARY diagnostic: GET an arbitrary Web API path and report what comes
/// back. Used to map which endpoints Spotify is currently refusing. Remove.
#[tauri::command]
pub async fn probe_webapi(state: State<'_, AppState>, path: String) -> AppResult<String> {
    let t = token(&state).await?;
    let v: serde_json::Value = WebApi::new().get(&t, &path, &[]).await?;
    Ok(v.to_string().chars().take(300).collect())
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
pub async fn get_recently_played(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::recently_played(&WebApi::new(), &t, limit.unwrap_or(50)).await
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
pub async fn get_tracks_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<bool>> {
    let t = token(&state).await?;
    library::tracks_saved(&WebApi::new(), &t, &ids).await
}

#[tauri::command]
pub async fn get_artist_top_tracks(
    state: State<'_, AppState>,
    artist_id: String,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::artist_top_tracks(&WebApi::new(), &t, &artist_id).await
}

#[tauri::command]
pub async fn get_artist_albums(
    state: State<'_, AppState>,
    artist_id: String,
) -> AppResult<Vec<AlbumSummary>> {
    let t = token(&state).await?;
    library::artist_albums(&WebApi::new(), &t, &artist_id).await
}

// ---- search -------------------------------------------------------------

#[tauri::command]
pub async fn search_spotify(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> AppResult<SearchResults> {
    let t = token(&state).await?;
    let limit = limit.unwrap_or(search::MAX_SEARCH_LIMIT);
    search::search(&WebApi::new(), &t, &query, limit).await
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
