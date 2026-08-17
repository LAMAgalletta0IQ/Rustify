use librespot::connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
use librespot::core::authentication::Credentials;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth;
use crate::connect::{self, Device};
use crate::error::{AppError, AppResult};
use crate::library::{self, AlbumSummary, PlaylistSummary, TrackSummary};
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
        .map(|s| format!("{s} (spotify-rust)"))
        .unwrap_or_else(|| "spotify-rust".to_string())
}

// ---- auth ---------------------------------------------------------------

#[tauri::command]
pub async fn get_auth_state(state: State<'_, AppState>) -> AppResult<AuthState> {
    Ok(state.auth.read().await.clone())
}

/// Completes a login given an OAuth token: verifies Premium, starts librespot,
/// persists the refresh token.
async fn establish(
    app: &AppHandle,
    state: &AppState,
    api: &WebApi,
    tok: librespot_oauth::OAuthToken,
) -> AppResult<AuthState> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    // Persist the refresh token *before* the Premium gate. The OAuth flow has
    // already succeeded by this point, so if the gate then fails on a
    // transient error (notably a 429 on /me), a retry can go through
    // `restore_session` silently instead of reopening the browser.
    auth::save_stored_tokens(
        &data_dir,
        &auth::StoredTokens {
            refresh_token: tok.refresh_token.clone(),
        },
    )?;

    // Premium gate, so a free account gets a clear message rather than a
    // silent playback failure later.
    let auth_state = auth::fetch_profile_require_premium(api, &tok.access_token).await?;

    // Publish the token before anything reads it.
    state.tokens.set(tok.access_token.clone()).await;

    let started = player::start_session(
        app.clone(),
        Credentials::with_access_token(tok.access_token.clone()),
        state.tokens.clone(),
        device_name(),
        data_dir.join("cache"),
    )
    .await?;

    // Access tokens last ~1h; without this every Web API call would start
    // failing mid-session.
    let refresh_task = auth::spawn_refresher(
        auth::default_client_id(),
        tok.refresh_token,
        state.tokens.clone(),
        data_dir,
        tok.expires_at,
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
    }

    *state.spotify.write().await = Some(SpotifySession {
        session: started.session,
        spirc: started.spirc,
        refresh_task,
    });
    *state.auth.write().await = auth_state.clone();

    let _ = app.emit(events::AUTH, &auth_state);
    Ok(auth_state)
}

#[tauri::command]
pub async fn login(app: AppHandle, state: State<'_, AppState>) -> AppResult<AuthState> {
    let api = WebApi::new();
    let client_id = auth::default_client_id();
    let tok = auth::interactive_login(&client_id).await?;
    establish(&app, &state, &api, tok).await
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
    let client_id = auth::default_client_id();

    match auth::refresh_login(&client_id, &stored.refresh_token).await {
        Ok(tok) => establish(&app, &state, &api, tok).await,
        Err(e) => {
            // Refresh token revoked/expired: drop it and fall back to the
            // login screen instead of surfacing a scary error.
            log::warn!("stored refresh token unusable: {e}");
            auth::clear_stored_tokens(&data_dir);
            Ok(AuthState::default())
        }
    }
}

#[tauri::command]
pub async fn logout(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    if let Some(s) = state.spotify.write().await.take() {
        let _ = s.spirc.shutdown();
        s.refresh_task.abort();
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
    with_spirc(&state, move |s| {
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
    with_spirc(&state, move |s| {
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
    search::search(&WebApi::new(), &t, &query, limit.unwrap_or(20)).await
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
