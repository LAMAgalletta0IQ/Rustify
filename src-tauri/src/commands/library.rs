//! Playlists, saved items, followed artists/releases, top tracks/artists,
//! artist detail pages, and search — the plain Web API + basic-Pathfinder
//! library surface.

use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, AppResult};
use crate::library::{
    self, AlbumPage, AlbumSummary, ArtistPage, FollowedReleasePage, PlaylistSummary,
    RecentActivityItem, TrackSummary,
};
use crate::search::{self, ArtistSummary, SearchResults};
use crate::state::AppState;

use super::token;

#[tauri::command]
pub async fn get_playlists(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<PlaylistSummary>> {
    let t = token(&state).await?;
    library::playlists(&state.web_api, &t, limit.unwrap_or(50), offset.unwrap_or(0)).await
}

/// Renames / re-describes a playlist the signed-in user owns.
///
/// Ownership is not re-checked here: Spotify answers 403 for a playlist the
/// token cannot modify, and that is the authoritative answer. The UI hides the
/// button for playlists it can already tell aren't the user's, which is a
/// convenience, not the security boundary.
#[tauri::command]
pub async fn update_playlist_details(
    state: State<'_, AppState>,
    playlist_id: String,
    name: Option<String>,
    description: Option<String>,
    public: Option<bool>,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::update_playlist_details(
        &state.web_api,
        &t,
        &playlist_id,
        name.as_deref(),
        description.as_deref(),
        public,
    )
    .await
}

/// Replaces a playlist cover. `jpegBase64` is base64 JPEG (a `data:` URL is
/// accepted too); the webview re-encodes whatever file the user picked so the
/// backend never has to depend on an image codec for this.
#[tauri::command]
pub async fn update_playlist_image(
    state: State<'_, AppState>,
    playlist_id: String,
    jpeg_base64: String,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::update_playlist_image(&state.web_api, &t, &playlist_id, &jpeg_base64).await
}

#[tauri::command]
pub async fn get_playlist_tracks(
    state: State<'_, AppState>,
    playlist_id: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    // Spotify's generated/personalized playlists (Daily Mix, Discover Weekly,
    // Release Radar, Daylist…) carry an ordinary spotify:playlist: URI
    // everywhere else, but the public REST endpoint 404s on their id
    // specifically — that's `Unavailable`. Since Spotify's February 2026 Web
    // API change, `/playlists/{id}/items` also answers 403 (`Forbidden`) for
    // any playlist the signed-in user does not own or collaborate on, public
    // or not — reading someone else's public playlist used to work over REST
    // and no longer does. Pathfinder's `fetchPlaylistContents` is the same
    // operation the Spotify web player itself calls and isn't subject to
    // either restriction, so both failures fall back to it. Any other error
    // (auth, rate limit, a genuinely missing playlist) is returned as-is
    // rather than masked by a second, different error.
    let (rest_message, reconstruct): (String, fn(String) -> AppError) =
        match library::playlist_tracks(&state.web_api, &t, &playlist_id, limit, offset).await {
            Err(AppError::Unavailable(msg)) => (msg, AppError::Unavailable),
            Err(AppError::Forbidden(msg)) => (msg, AppError::Forbidden),
            other => return other,
        };

    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    match state
        .internal_spotify
        .playlist_contents(&session, &playlist_id, limit, offset)
        .await
    {
        Ok(page) => Ok(page.tracks),
        Err(error) => {
            log::warn!(
                target: "spotify.playlist",
                "playlist {playlist_id} failed via REST ({rest_message}) and Pathfinder fallback also failed: {error}"
            );
            Err(reconstruct(rest_message))
        }
    }
}

#[tauri::command]
pub async fn get_saved_tracks(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::saved_tracks(&state.web_api, &t, limit.unwrap_or(50), offset.unwrap_or(0)).await
}

#[tauri::command]
pub async fn get_saved_albums(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<AlbumSummary>> {
    let t = token(&state).await?;
    library::saved_albums(&state.web_api, &t, limit.unwrap_or(50), offset.unwrap_or(0)).await
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
    library::followed_artists(&state.web_api, &t, limit.unwrap_or(50), after.as_deref()).await
}

#[tauri::command]
pub async fn get_followed_releases(
    state: State<'_, AppState>,
    after: Option<String>,
) -> AppResult<FollowedReleasePage> {
    let t = token(&state).await?;
    library::followed_releases(&state.web_api, &t, after.as_deref()).await
}

#[tauri::command]
pub async fn get_recently_played(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<RecentActivityItem>> {
    let t = token(&state).await?;
    library::recently_played(&state.web_api, &t, limit.unwrap_or(50)).await
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
    library::album_tracks(&state.web_api, &t, &album_id).await
}

#[tauri::command]
pub async fn set_tracks_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
    saved: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::set_tracks_saved(&state.web_api, &t, &ids, saved).await?;
    library::invalidate_saved_tracks_cache(&state.saved_tracks_cache).await;
    Ok(())
}

#[tauri::command]
pub async fn set_albums_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
    saved: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::set_albums_saved(&state.web_api, &t, &ids, saved).await
}

#[tauri::command]
pub async fn set_artists_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
    saved: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    library::set_artists_saved(&state.web_api, &t, &ids, saved).await
}

#[tauri::command]
pub async fn get_tracks_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<bool>> {
    let t = token(&state).await?;
    library::tracks_saved(&state.web_api, &t, &ids).await
}

#[tauri::command]
pub async fn get_albums_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<bool>> {
    let t = token(&state).await?;
    library::albums_saved(&state.web_api, &t, &ids).await
}

#[tauri::command]
pub async fn get_artists_saved(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> AppResult<Vec<bool>> {
    let t = token(&state).await?;
    library::artists_saved(&state.web_api, &t, &ids).await
}

#[tauri::command]
pub async fn get_liked_tracks_by_artist(
    state: State<'_, AppState>,
    artist_id: String,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::liked_tracks_by_artist(&state.web_api, &t, &artist_id, &state.saved_tracks_cache).await
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
        &state.web_api,
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
    library::artist(&state.web_api, &t, &artist_id).await
}

/// Stats, top tracks and concerts in one call. Replaces what used to be a
/// REST-only top-tracks implementation that fetched five albums and then
/// every track of each — `/artists/{id}/top-tracks` was retired in
/// February 2026, so that fan-out was the whole implementation, not a cache
/// miss path. This uses the same `queryArtistOverview` Pathfinder operation
/// the concerts feature already calls; if its topTracks field ever goes
/// empty (a private, undocumented API can rename fields under us) this falls
/// back to the old REST reconstruction rather than showing nothing.
#[tauri::command]
pub async fn get_artist_overview(
    state: State<'_, AppState>,
    artist_id: String,
    locale: Option<String>,
) -> AppResult<crate::spotify::ArtistOverview> {
    const ARTIST_OVERVIEW_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(600);
    const MAX_ARTIST_OVERVIEW_CACHE_ENTRIES: usize = 64;

    let cache_key = format!("{artist_id}:{}", locale.as_deref().unwrap_or(""));
    if let Some((fetched_at, cached)) = state.artist_overview_cache.read().await.get(&cache_key) {
        if fetched_at.elapsed() < ARTIST_OVERVIEW_CACHE_TTL {
            return Ok(cached.clone());
        }
    }

    // Gate concurrent requests for the same artist (e.g. a double-click, or
    // navigating away and immediately back) onto one Pathfinder call rather
    // than firing one per caller — same pattern as `lyrics_requests`.
    let request_gate = {
        let mut requests = state.artist_overview_requests.lock().await;
        if requests.len() >= MAX_ARTIST_OVERVIEW_CACHE_ENTRIES && !requests.contains_key(&cache_key)
        {
            requests.clear();
        }
        requests
            .entry(cache_key.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _request_guard = request_gate.lock().await;
    if let Some((fetched_at, cached)) = state.artist_overview_cache.read().await.get(&cache_key) {
        if fetched_at.elapsed() < ARTIST_OVERVIEW_CACHE_TTL {
            return Ok(cached.clone());
        }
    }

    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    // Whether the *field* was empty inside a successful Pathfinder response
    // (schema drift — worth reconstructing from REST) versus the whole
    // Pathfinder call having failed (rate-limited/unreachable/hash-rejected —
    // reconstructing from REST would only pile more requests, up to 1+5 of
    // them, onto a service that just told us to back off). These used to
    // collapse into the same "top_tracks.is_empty()" state below, so a single
    // Pathfinder 429 on this operation triggered a 6-request REST fallback on
    // every artist page opened until the rate limit cleared — amplifying the
    // exact 429 that caused it in the first place.
    let pathfinder_failed;
    let mut overview = match state
        .internal_spotify
        .artist_overview(&session, &artist_id, locale.as_deref().unwrap_or(""))
        .await
    {
        Ok(overview) => {
            pathfinder_failed = false;
            overview
        }
        Err(error) => {
            pathfinder_failed = true;
            log::warn!(
                target: "spotify.artist",
                "artist overview via Pathfinder failed for {artist_id}: {error}"
            );
            crate::spotify::ArtistOverview {
                stats: Default::default(),
                top_tracks: Vec::new(),
                concerts: crate::spotify::ConcertFeed::unavailable(),
            }
        }
    };
    if overview.top_tracks.is_empty() && !pathfinder_failed {
        let t = token(&state).await?;
        match library::artist_tracks(&state.web_api, &t, &artist_id).await {
            Ok(fallback) => overview.top_tracks = fallback,
            Err(error) => log::warn!(
                target: "spotify.artist",
                "Pathfinder top tracks empty and REST fallback failed for {artist_id}: {error}"
            ),
        }
    }

    // Only a genuine Pathfinder success is worth remembering — caching the
    // empty placeholder from a failure would make a transient 429 look like
    // "this artist really has no top tracks" for the next ten minutes.
    if !pathfinder_failed {
        let mut cache = state.artist_overview_cache.write().await;
        if cache.len() >= MAX_ARTIST_OVERVIEW_CACHE_ENTRIES && !cache.contains_key(&cache_key) {
            cache.clear();
        }
        cache.insert(cache_key, (std::time::Instant::now(), overview.clone()));
    }
    Ok(overview)
}

#[tauri::command]
pub async fn get_top_tracks(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<TrackSummary>> {
    let t = token(&state).await?;
    library::top_tracks(&state.web_api, &t, limit.unwrap_or(20)).await
}

#[tauri::command]
pub async fn get_top_artists(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> AppResult<Vec<ArtistSummary>> {
    let t = token(&state).await?;
    library::top_artists(&state.web_api, &t, limit.unwrap_or(10)).await
}

// ---- search -----------------------------------------------------------

#[tauri::command]
pub async fn search_spotify(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<SearchResults> {
    let t = token(&state).await?;
    let limit = limit.unwrap_or(search::MAX_SEARCH_LIMIT);
    search::search(&state.web_api, &t, &query, limit, offset.unwrap_or(0)).await
}
