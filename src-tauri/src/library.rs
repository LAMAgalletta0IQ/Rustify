use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::error::AppResult;
use crate::search::ArtistSummary;
use crate::webapi::WebApi;

/// How long a cached saved-tracks snapshot is trusted before re-fetching.
/// Saves are rare relative to page views, and any explicit save/unsave
/// invalidates the cache immediately anyway — this bound only covers passive
/// drift from another device liking a track mid-session.
const SAVED_TRACKS_CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct SavedTracksSnapshot {
    tracks: Vec<TrackSummary>,
    fetched_at: Instant,
}

/// Flattened shape the UI renders. Keeping the Web API's nested envelopes out
/// of the frontend keeps serialisation cheap and the Svelte components dumb.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistSummary {
    pub uri: String,
    pub id: String,
    pub name: String,
    pub owner: String,
    pub image_url: Option<String>,
    pub track_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumSummary {
    pub uri: String,
    pub id: String,
    pub name: String,
    pub artists: Vec<String>,
    pub image_url: Option<String>,
    pub album_type: String,
    pub release_date: Option<String>,
    pub release_date_precision: Option<String>,
    pub total_tracks: Option<u32>,
}

/// A page of followed artists plus the cursor for the next one, since
/// `/me/following` cannot be paged by offset. `next` is `None` at the end.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistPage {
    pub items: Vec<ArtistSummary>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumPage {
    pub items: Vec<AlbumSummary>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowedReleasePage {
    pub items: Vec<AlbumSummary>,
    pub next_artist: Option<String>,
    /// A page can remain useful when one artist request is unavailable or
    /// rate-limited. The UI presents this as partial, never exhaustive.
    pub partial_errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentActivityItem {
    /// `track`, `album`, `playlist`, or `artist`.
    pub kind: String,
    pub uri: String,
    pub id: String,
    pub name: String,
    pub subtitle: String,
    pub image_url: Option<String>,
    pub last_played_at: String,
    /// Track to start at when the item represents a playable context.
    pub track_uri: Option<String>,
    /// Number of occurrences in the Web API history window.
    #[serde(default = "one")]
    pub frequency: u32,
}

const fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackSummary {
    pub uri: String,
    pub id: String,
    pub name: String,
    pub artists: Vec<String>,
    pub artist_ids: Vec<String>,
    pub album: String,
    pub image_url: Option<String>,
    pub duration_ms: u32,
    pub explicit: bool,
}

// ---- Web API wire types -------------------------------------------------

#[derive(Debug, Deserialize)]
struct Page<T> {
    items: Vec<T>,
    #[serde(default)]
    total: Option<u32>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WirePlaylist {
    id: String,
    uri: String,
    name: String,
    owner: WireNamed,
    images: Option<Vec<WireImage>>,
    /// Spotify renamed this object on `/me/playlists`: it is now `items`
    /// (`{href, total}`), not `tracks`. Verified live 2026-08-17 — the response
    /// carries no `tracks` key at all, which silently made every playlist show
    /// "0 tracks". The alias accepts both so either shape keeps working.
    #[serde(alias = "items")]
    tracks: Option<WireTrackRef>,
}

#[derive(Debug, Deserialize)]
struct WireTrackRef {
    total: u32,
}

#[derive(Debug, Deserialize)]
struct WireNamed {
    #[serde(alias = "name")]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireImage {
    url: String,
    width: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct WireArtist {
    #[serde(default)]
    id: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct WireAlbum {
    id: String,
    uri: String,
    name: String,
    artists: Vec<WireArtist>,
    images: Vec<WireImage>,
    #[serde(default)]
    album_type: String,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    release_date_precision: Option<String>,
    #[serde(default)]
    total_tracks: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct WireTrack {
    id: Option<String>,
    uri: String,
    name: String,
    artists: Vec<WireArtist>,
    album: Option<WireAlbum>,
    duration_ms: u32,
    #[serde(default)]
    explicit: bool,
}

#[derive(Debug, Deserialize)]
struct SavedAlbum {
    album: WireAlbum,
}

#[derive(Debug, Deserialize)]
struct SavedTrack {
    track: WireTrack,
}

#[derive(Debug, Deserialize)]
struct PlaylistItem {
    /// Renamed alongside the endpoint: `/playlists/{id}/items` returns rows
    /// keyed `item`, not `track`. Verified live 2026-08-17 —
    /// `fields=items(track(name))` comes back `{"items":[{}]}` while
    /// `fields=items(item(name))` returns the track. Missing the alias parsed
    /// every row to `None`, so playlists opened to "Nothing here." with no
    /// error logged anywhere.
    #[serde(alias = "item")]
    track: Option<WireTrack>,
}

#[derive(Debug, Deserialize)]
struct PlayHistoryItem {
    track: WireTrack,
    played_at: String,
    context: Option<WireContext>,
}

#[derive(Debug, Deserialize)]
struct WireContext {
    #[serde(rename = "type")]
    kind: String,
    uri: String,
}

/// `/me/following` wraps its page in an extra object; nothing else here does.
#[derive(Debug, Deserialize)]
struct FollowedArtists {
    artists: CursorPage<WireFullArtist>,
}

#[derive(Debug, Deserialize)]
struct CursorPage<T> {
    items: Vec<T>,
    cursors: Option<Cursors>,
}

#[derive(Debug, Deserialize)]
struct Cursors {
    after: Option<String>,
}

/// Full artist object — unlike the stub inside a track, this one carries images.
#[derive(Debug, Deserialize)]
struct WireFullArtist {
    id: String,
    uri: String,
    name: String,
    images: Option<Vec<WireImage>>,
}

fn pick_image(images: &[WireImage]) -> Option<String> {
    images
        .iter()
        .min_by_key(|i| (i.width.unwrap_or(640) as i32 - 300).abs())
        .map(|i| i.url.clone())
}

impl From<WireTrack> for TrackSummary {
    fn from(t: WireTrack) -> Self {
        let artist_ids = t.artists.iter().filter_map(|a| a.id.clone()).collect();
        TrackSummary {
            id: t.id.unwrap_or_default(),
            uri: t.uri,
            name: t.name,
            artists: t.artists.into_iter().map(|a| a.name).collect(),
            artist_ids,
            album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            image_url: t.album.as_ref().and_then(|a| pick_image(&a.images)),
            duration_ms: t.duration_ms,
            explicit: t.explicit,
        }
    }
}

impl From<WireAlbum> for AlbumSummary {
    fn from(a: WireAlbum) -> Self {
        AlbumSummary {
            image_url: pick_image(&a.images),
            id: a.id,
            uri: a.uri,
            name: a.name,
            artists: a.artists.into_iter().map(|x| x.name).collect(),
            album_type: a.album_type,
            release_date: a.release_date,
            release_date_precision: a.release_date_precision,
            total_tracks: a.total_tracks,
        }
    }
}

// ---- Public API ---------------------------------------------------------

pub async fn playlists(
    api: &WebApi,
    token: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<PlaylistSummary>> {
    let page: Page<WirePlaylist> = api
        .get(
            token,
            "/me/playlists",
            &[
                ("limit", limit.min(50).to_string()),
                ("offset", offset.to_string()),
            ],
        )
        .await?;

    Ok(page
        .items
        .into_iter()
        .map(|p| PlaylistSummary {
            image_url: p.images.as_deref().and_then(pick_image),
            track_count: p.tracks.map(|t| t.total).unwrap_or(0),
            owner: p.owner.display_name.unwrap_or_default(),
            id: p.id,
            uri: p.uri,
            name: p.name,
        })
        .collect())
}

pub async fn playlist_tracks(
    api: &WebApi,
    token: &str,
    playlist_id: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<TrackSummary>> {
    let page: Page<PlaylistItem> = api
        .get(
            token,
            // `/tracks` now answers 403 Forbidden (observed live 2026-08-17,
            // on a private client ID with playlist-read-private granted).
            // `/me/playlists` itself hands back `items.href` pointing at
            // `/playlists/{id}/items`, so that is the path Spotify expects now.
            &format!("/playlists/{playlist_id}/items"),
            &[
                ("limit", limit.min(100).to_string()),
                ("offset", offset.to_string()),
            ],
        )
        .await?;

    // Locally-added / unavailable rows come back with a null track.
    Ok(page
        .items
        .into_iter()
        .filter_map(|i| i.track)
        .map(Into::into)
        .collect())
}

pub async fn saved_tracks(
    api: &WebApi,
    token: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<TrackSummary>> {
    let page: Page<SavedTrack> = api
        .get(
            token,
            "/me/tracks",
            &[
                ("limit", limit.min(50).to_string()),
                ("offset", offset.to_string()),
            ],
        )
        .await?;
    Ok(page.items.into_iter().map(|s| s.track.into()).collect())
}

pub async fn saved_albums(
    api: &WebApi,
    token: &str,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<AlbumSummary>> {
    let page: Page<SavedAlbum> = api
        .get(
            token,
            "/me/albums",
            &[
                ("limit", limit.min(50).to_string()),
                ("offset", offset.to_string()),
            ],
        )
        .await?;
    Ok(page.items.into_iter().map(|s| s.album.into()).collect())
}

pub async fn album_tracks(
    api: &WebApi,
    token: &str,
    album_id: &str,
) -> AppResult<Vec<TrackSummary>> {
    let page: Page<WireTrack> = api
        .get(
            token,
            &format!("/albums/{album_id}/tracks"),
            &[("limit", "50".to_string())],
        )
        .await?;
    Ok(page.items.into_iter().map(Into::into).collect())
}

// ---- saving / unsaving --------------------------------------------------

/// `PUT`/`DELETE /me/tracks` and `/me/albums` take `{"ids": [...]}`. Spotify
/// caps these at 50 ids per call; callers here work in much smaller batches.
pub async fn set_tracks_saved(
    api: &WebApi,
    token: &str,
    ids: &[String],
    saved: bool,
) -> AppResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    set_library_saved(api, token, ids, "track", saved).await
}

pub async fn set_albums_saved(
    api: &WebApi,
    token: &str,
    ids: &[String],
    saved: bool,
) -> AppResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    set_library_saved(api, token, ids, "album", saved).await
}

pub async fn set_artists_saved(
    api: &WebApi,
    token: &str,
    ids: &[String],
    saved: bool,
) -> AppResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    set_library_saved(api, token, ids, "artist", saved).await
}

async fn set_library_saved(
    api: &WebApi,
    token: &str,
    ids: &[String],
    kind: &str,
    saved: bool,
) -> AppResult<()> {
    for chunk in ids.chunks(40) {
        let uris = chunk
            .iter()
            .map(|id| format!("spotify:{kind}:{id}"))
            .collect::<Vec<_>>()
            .join(",");
        let query = [("uris", uris)];
        if saved {
            api.put_query(token, "/me/library", &query).await?;
        } else {
            api.delete_query(token, "/me/library", &query).await?;
        }
    }
    Ok(())
}

/// Returns one bool per id, in the order given.
pub async fn tracks_saved(api: &WebApi, token: &str, ids: &[String]) -> AppResult<Vec<bool>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    library_saved(api, token, ids, "track").await
}

pub async fn albums_saved(api: &WebApi, token: &str, ids: &[String]) -> AppResult<Vec<bool>> {
    library_saved(api, token, ids, "album").await
}

pub async fn artists_saved(api: &WebApi, token: &str, ids: &[String]) -> AppResult<Vec<bool>> {
    library_saved(api, token, ids, "artist").await
}

pub async fn liked_tracks_by_artist(
    api: &WebApi,
    token: &str,
    artist_id: &str,
    cache: &RwLock<Option<SavedTracksSnapshot>>,
) -> AppResult<Vec<TrackSummary>> {
    let tracks = saved_tracks_snapshot(api, token, cache).await?;
    Ok(tracks
        .into_iter()
        .filter(|track| track.artist_ids.iter().any(|id| id == artist_id))
        .collect())
}

/// Every saved track, from cache when fresh. A cold or expired cache costs the
/// same bounded walk `liked_tracks_by_artist` always did; a warm one costs
/// nothing, which is what makes browsing several artists in a row affordable.
async fn saved_tracks_snapshot(
    api: &WebApi,
    token: &str,
    cache: &RwLock<Option<SavedTracksSnapshot>>,
) -> AppResult<Vec<TrackSummary>> {
    if let Some(snapshot) = cache.read().await.as_ref() {
        if snapshot.fetched_at.elapsed() < SAVED_TRACKS_CACHE_TTL {
            return Ok(snapshot.tracks.clone());
        }
    }
    let mut result = Vec::new();
    // Bound the walk so an enormous library cannot monopolize the API quota.
    // The UI describes the result as the checked portion when this cap is hit.
    for offset in (0..500).step_by(50) {
        let page = saved_tracks(api, token, 50, offset).await?;
        let count = page.len();
        result.extend(page);
        if count < 50 {
            break;
        }
    }
    *cache.write().await = Some(SavedTracksSnapshot {
        tracks: result.clone(),
        fetched_at: Instant::now(),
    });
    Ok(result)
}

/// Drops the saved-tracks snapshot. Call after any save/unsave so the next
/// artist page reflects it immediately instead of waiting out the TTL.
pub async fn invalidate_saved_tracks_cache(cache: &RwLock<Option<SavedTracksSnapshot>>) {
    *cache.write().await = None;
}

async fn library_saved(
    api: &WebApi,
    token: &str,
    ids: &[String],
    kind: &str,
) -> AppResult<Vec<bool>> {
    let mut result = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(40) {
        let uris = chunk
            .iter()
            .map(|id| format!("spotify:{kind}:{id}"))
            .collect::<Vec<_>>()
            .join(",");
        let values: Vec<bool> = api
            .get(token, "/me/library/contains", &[("uris", uris)])
            .await?;
        result.extend(values);
    }
    Ok(result)
}

// ---- artist -------------------------------------------------------------

pub async fn artist_tracks(
    api: &WebApi,
    token: &str,
    artist_id: &str,
) -> AppResult<Vec<TrackSummary>> {
    // `/artists/{id}/top-tracks` was removed in February 2026. Build a
    // truthful, deterministic sample from the artist's recent releases using
    // endpoints that remain supported; the UI labels it accordingly.
    let releases = artist_albums(api, token, artist_id, 5, 0).await?;
    let mut tracks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for album in releases.items {
        for mut track in album_tracks(api, token, &album.id).await? {
            track.album = album.name.clone();
            track.image_url = album.image_url.clone();
            if seen.insert(track.uri.clone()) {
                tracks.push(track);
            }
            if tracks.len() >= 10 {
                return Ok(tracks);
            }
        }
    }
    Ok(tracks)
}

pub async fn artist_albums(
    api: &WebApi,
    token: &str,
    artist_id: &str,
    limit: u32,
    offset: u32,
) -> AppResult<AlbumPage> {
    let page: Page<WireAlbum> = api
        .get(
            token,
            &format!("/artists/{artist_id}/albums"),
            &[
                ("include_groups", "album,single".to_string()),
                ("limit", limit.min(10).to_string()),
                ("offset", offset.to_string()),
            ],
        )
        .await?;
    let count = page.items.len() as u32;
    let has_more = page.next.is_some()
        || page
            .total
            .is_some_and(|total| offset.saturating_add(count) < total);
    Ok(AlbumPage {
        items: page.items.into_iter().map(Into::into).collect(),
        has_more,
    })
}

pub async fn artist(api: &WebApi, token: &str, artist_id: &str) -> AppResult<ArtistSummary> {
    let a: WireFullArtist = api
        .get(token, &format!("/artists/{artist_id}"), &[])
        .await?;
    Ok(ArtistSummary {
        image_url: pick_image(a.images.as_deref().unwrap_or_default()),
        id: a.id,
        uri: a.uri,
        name: a.name,
    })
}

pub async fn top_tracks(api: &WebApi, token: &str, limit: u32) -> AppResult<Vec<TrackSummary>> {
    let page: Page<WireTrack> = api
        .get(
            token,
            "/me/top/tracks",
            &[
                ("limit", limit.min(50).to_string()),
                ("time_range", "medium_term".to_string()),
            ],
        )
        .await?;
    Ok(page.items.into_iter().map(Into::into).collect())
}

pub async fn top_artists(api: &WebApi, token: &str, limit: u32) -> AppResult<Vec<ArtistSummary>> {
    let page: Page<WireFullArtist> = api
        .get(
            token,
            "/me/top/artists",
            &[
                ("limit", limit.min(50).to_string()),
                ("time_range", "medium_term".to_string()),
            ],
        )
        .await?;
    Ok(page
        .items
        .into_iter()
        .map(|a| ArtistSummary {
            image_url: pick_image(a.images.as_deref().unwrap_or_default()),
            id: a.id,
            uri: a.uri,
            name: a.name,
        })
        .collect())
}

/// Artists the user follows.
///
/// The odd one out in this module: `/me/following` is **cursor**-paginated
/// rather than offset-paginated, so the caller must hand back the `next`
/// cursor from the previous page instead of counting how many items it holds.
/// It also nests its page under an `artists` key rather than returning the
/// page object at the top level, hence the extra wrapper.
pub async fn followed_artists(
    api: &WebApi,
    token: &str,
    limit: u32,
    after: Option<&str>,
) -> AppResult<ArtistPage> {
    let mut query = vec![
        ("type", "artist".to_string()),
        ("limit", limit.min(50).to_string()),
    ];
    if let Some(cursor) = after {
        query.push(("after", cursor.to_string()));
    }

    let resp: FollowedArtists = api.get(token, "/me/following", &query).await?;
    Ok(ArtistPage {
        next: resp.artists.cursors.and_then(|c| c.after),
        items: resp
            .artists
            .items
            .into_iter()
            .map(|a| ArtistSummary {
                image_url: pick_image(a.images.as_deref().unwrap_or_default()),
                id: a.id,
                uri: a.uri,
                name: a.name,
            })
            .collect(),
    })
}

/// Recent catalog releases for a cursor page of followed artists. Spotify has
/// no aggregate endpoint, so pagination is deliberately by followed-artist
/// cursor and every partial failure is surfaced.
pub async fn followed_releases(
    api: &WebApi,
    token: &str,
    after: Option<&str>,
) -> AppResult<FollowedReleasePage> {
    let artists = followed_artists(api, token, 8, after).await?;
    let mut items = Vec::new();
    let mut partial_errors = Vec::new();
    // Sequential requests deliberately avoid an eight-request burst against
    // Spotify's per-client rolling quota. A page is small enough that this
    // remains responsive, and partial results survive any one failure.
    for artist in &artists.items {
        match artist_albums(api, token, &artist.id, 10, 0).await {
            Ok(page) => items.extend(page.items),
            Err(error @ crate::error::AppError::RateLimited { .. })
            | Err(error @ crate::error::AppError::SessionExpired) => return Err(error),
            Err(error) => partial_errors.push(format!("{}: {error}", artist.name)),
        }
    }
    let items = deduplicate_releases(items);
    Ok(FollowedReleasePage {
        items,
        next_artist: artists.next,
        partial_errors,
    })
}

fn edition_key(album: &AlbumSummary) -> String {
    let mut title = album.name.to_lowercase();
    for suffix in [
        "(deluxe edition)",
        "(deluxe)",
        "[deluxe edition]",
        "- deluxe edition",
        "(remastered)",
        "(remaster)",
    ] {
        title = title.replace(suffix, "");
    }
    let year = album
        .release_date
        .as_deref()
        .unwrap_or("")
        .get(..4)
        .unwrap_or("");
    format!(
        "{}|{}|{}|{}",
        album
            .artists
            .first()
            .map(|name| name.to_lowercase())
            .unwrap_or_default(),
        title.trim(),
        album.album_type,
        year
    )
}

fn deduplicate_releases(items: Vec<AlbumSummary>) -> Vec<AlbumSummary> {
    let mut chosen = std::collections::HashMap::<String, AlbumSummary>::new();
    for album in items {
        let key = edition_key(&album);
        chosen
            .entry(key)
            .and_modify(|existing| {
                // Prefer the fuller edition, then a record with artwork/date.
                let candidate = (
                    album.total_tracks.unwrap_or(0),
                    album.image_url.is_some(),
                    album.release_date.is_some(),
                );
                let current = (
                    existing.total_tracks.unwrap_or(0),
                    existing.image_url.is_some(),
                    existing.release_date.is_some(),
                );
                if candidate > current {
                    *existing = album.clone();
                }
            })
            .or_insert(album);
    }
    let mut result = chosen.into_values().collect::<Vec<_>>();
    result.sort_by(|a, b| {
        b.release_date
            .cmp(&a.release_date)
            .then_with(|| a.name.cmp(&b.name))
    });
    result
}

/// The 50 most recently played tracks, newest first.
///
/// History contains one entry per *play*, so a track on repeat fills the whole
/// response. Deduplicated by URI here — a "recently played" shelf showing the
/// same album six times is worse than showing six items.
pub async fn recently_played(
    api: &WebApi,
    token: &str,
    limit: u32,
) -> AppResult<Vec<RecentActivityItem>> {
    let page: Page<PlayHistoryItem> = api
        .get(
            token,
            "/me/player/recently-played",
            &[("limit", limit.min(50).to_string())],
        )
        .await?;

    Ok(collapse_history(page.items))
}

fn collapse_history(history_items: Vec<PlayHistoryItem>) -> Vec<RecentActivityItem> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for history in history_items {
        let track_uri = history.track.uri.clone();
        let track = TrackSummary::from(history.track);
        let activity = match history.context {
            Some(context) if context.kind == "album" => {
                let id = context
                    .uri
                    .rsplit(':')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                RecentActivityItem {
                    kind: "album".into(),
                    uri: context.uri,
                    id,
                    name: track.album.clone(),
                    subtitle: track.artists.join(", "),
                    image_url: track.image_url.clone(),
                    last_played_at: history.played_at,
                    track_uri: Some(track_uri),
                    frequency: 1,
                }
            }
            Some(context) if context.kind == "artist" => {
                let id = context
                    .uri
                    .rsplit(':')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                RecentActivityItem {
                    kind: "artist".into(),
                    uri: context.uri,
                    id,
                    name: track
                        .artists
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Artist".into()),
                    subtitle: "Artist".into(),
                    image_url: None,
                    last_played_at: history.played_at,
                    track_uri: Some(track_uri),
                    frequency: 1,
                }
            }
            // Playlist context metadata is not embedded in playback history.
            // Keep the real context playable and label it honestly rather than
            // inventing a playlist name or making one request per history row.
            Some(context) if context.kind == "playlist" => {
                let id = context
                    .uri
                    .rsplit(':')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                RecentActivityItem {
                    kind: "playlist".into(),
                    uri: context.uri,
                    id,
                    name: "Recently played playlist".into(),
                    subtitle: format!("Last played track: {}", track.name),
                    image_url: track.image_url.clone(),
                    last_played_at: history.played_at,
                    track_uri: Some(track_uri),
                    frequency: 1,
                }
            }
            _ => RecentActivityItem {
                kind: "track".into(),
                uri: track.uri.clone(),
                id: track.id.clone(),
                name: track.name.clone(),
                subtitle: track.artists.join(", "),
                image_url: track.image_url.clone(),
                last_played_at: history.played_at,
                track_uri: None,
                frequency: 1,
            },
        };
        if seen.insert(activity.uri.clone()) {
            result.push(activity);
        } else if let Some(existing) = result.iter_mut().find(|item| item.uri == activity.uri) {
            existing.frequency = existing.frequency.saturating_add(1);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_library_uris_are_stable() {
        let ids = ["abc".to_string(), "def".to_string()];
        let uris = ids
            .iter()
            .map(|id| format!("spotify:track:{id}"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(uris, "spotify:track:abc,spotify:track:def");
    }

    fn wire_track(id: &str, name: &str, album_id: &str) -> WireTrack {
        WireTrack {
            id: Some(id.into()),
            uri: format!("spotify:track:{id}"),
            name: name.into(),
            artists: vec![WireArtist {
                id: Some("artist".into()),
                name: "Artist".into(),
            }],
            album: Some(WireAlbum {
                id: album_id.into(),
                uri: format!("spotify:album:{album_id}"),
                name: "Album".into(),
                artists: vec![WireArtist {
                    id: Some("artist".into()),
                    name: "Artist".into(),
                }],
                images: vec![],
                album_type: "album".into(),
                release_date: Some("2026-01-01".into()),
                release_date_precision: Some("day".into()),
                total_tracks: Some(10),
            }),
            duration_ms: 180_000,
            explicit: false,
        }
    }

    #[test]
    fn recent_activity_groups_contexts_and_keeps_latest_order() {
        let rows = vec![
            PlayHistoryItem {
                track: wire_track("one", "One", "album"),
                played_at: "2026-08-18T12:00:00Z".into(),
                context: Some(WireContext {
                    kind: "album".into(),
                    uri: "spotify:album:album".into(),
                }),
            },
            PlayHistoryItem {
                track: wire_track("two", "Two", "album"),
                played_at: "2026-08-18T11:59:00Z".into(),
                context: Some(WireContext {
                    kind: "album".into(),
                    uri: "spotify:album:album".into(),
                }),
            },
            PlayHistoryItem {
                track: wire_track("loose", "Loose", "single"),
                played_at: "2026-08-18T11:58:00Z".into(),
                context: None,
            },
            PlayHistoryItem {
                track: wire_track("loose", "Loose", "single"),
                played_at: "2026-08-18T11:57:00Z".into(),
                context: None,
            },
        ];

        let result = collapse_history(rows);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].kind, "album");
        assert_eq!(result[0].track_uri.as_deref(), Some("spotify:track:one"));
        assert_eq!(result[1].kind, "track");
        assert_eq!(result[1].uri, "spotify:track:loose");
    }

    #[test]
    fn release_editions_are_deduplicated_but_distinct_years_remain() {
        let release = |id: &str, name: &str, date: &str, tracks| AlbumSummary {
            uri: format!("spotify:album:{id}"),
            id: id.into(),
            name: name.into(),
            artists: vec!["Artist".into()],
            image_url: None,
            album_type: "album".into(),
            release_date: Some(date.into()),
            release_date_precision: Some("day".into()),
            total_tracks: Some(tracks),
        };
        let result = deduplicate_releases(vec![
            release("a", "Record", "2026-01-01", 10),
            release("b", "Record (Deluxe Edition)", "2026-02-01", 14),
            release("c", "Record", "2018-01-01", 10),
        ]);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|album| album.id == "b"));
        assert!(result.iter().any(|album| album.id == "c"));
    }
}
