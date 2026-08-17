use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::search::ArtistSummary;
use crate::webapi::WebApi;

/// Flattened shape the UI renders. Keeping the Web API's nested envelopes out
/// of the frontend keeps serialisation cheap and the Svelte components dumb.
#[derive(Debug, Clone, Serialize)]
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
pub struct TrackSummary {
    pub uri: String,
    pub id: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub image_url: Option<String>,
    pub duration_ms: u32,
}

// ---- Web API wire types -------------------------------------------------

#[derive(Debug, Deserialize)]
struct Page<T> {
    items: Vec<T>,
    #[allow(dead_code)]
    total: Option<u32>,
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
    name: String,
}

#[derive(Debug, Deserialize)]
struct WireAlbum {
    id: String,
    uri: String,
    name: String,
    artists: Vec<WireArtist>,
    images: Vec<WireImage>,
}

#[derive(Debug, Deserialize)]
struct WireTrack {
    id: Option<String>,
    uri: String,
    name: String,
    artists: Vec<WireArtist>,
    album: Option<WireAlbum>,
    duration_ms: u32,
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
    track: Option<WireTrack>,
}

#[derive(Debug, Deserialize)]
struct PlayHistoryItem {
    track: WireTrack,
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
        TrackSummary {
            id: t.id.unwrap_or_default(),
            uri: t.uri,
            name: t.name,
            artists: t.artists.into_iter().map(|a| a.name).collect(),
            album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            image_url: t.album.as_ref().and_then(|a| pick_image(&a.images)),
            duration_ms: t.duration_ms,
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
    let body = serde_json::json!({ "ids": ids });
    if saved {
        api.put(token, "/me/tracks", body).await
    } else {
        api.delete(token, "/me/tracks", body).await
    }
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
    let body = serde_json::json!({ "ids": ids });
    if saved {
        api.put(token, "/me/albums", body).await
    } else {
        api.delete(token, "/me/albums", body).await
    }
}

/// Returns one bool per id, in the order given.
pub async fn tracks_saved(
    api: &WebApi,
    token: &str,
    ids: &[String],
) -> AppResult<Vec<bool>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    match api
        .get(token, "/me/tracks/contains", &[("ids", ids.join(","))])
        .await
    {
        Ok(v) => Ok(v),
        // Spotify currently refuses the whole `*/contains` family with 403,
        // even though `/me/tracks` itself succeeds on the same token. The
        // saved-state is only the heart icon's fill, so answering "unknown"
        // keeps every track list usable instead of failing it outright with a
        // banner the user can do nothing about.
        Err(AppError::Forbidden(msg)) => {
            log::warn!("/me/tracks/contains refused ({msg}); showing tracks as unsaved");
            Ok(vec![false; ids.len()])
        }
        Err(e) => Err(e),
    }
}

// ---- artist -------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TopTracks {
    tracks: Vec<WireTrack>,
}

pub async fn artist_top_tracks(
    api: &WebApi,
    token: &str,
    artist_id: &str,
) -> AppResult<Vec<TrackSummary>> {
    let resp: TopTracks = api
        .get(token, &format!("/artists/{artist_id}/top-tracks"), &[])
        .await?;
    Ok(resp.tracks.into_iter().map(Into::into).collect())
}

pub async fn artist_albums(
    api: &WebApi,
    token: &str,
    artist_id: &str,
) -> AppResult<Vec<AlbumSummary>> {
    let page: Page<WireAlbum> = api
        .get(
            token,
            &format!("/artists/{artist_id}/albums"),
            &[
                ("include_groups", "album,single".to_string()),
                ("limit", "50".to_string()),
            ],
        )
        .await?;
    Ok(page.items.into_iter().map(Into::into).collect())
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

/// The 50 most recently played tracks, newest first.
///
/// History contains one entry per *play*, so a track on repeat fills the whole
/// response. Deduplicated by URI here — a "recently played" shelf showing the
/// same album six times is worse than showing six items.
pub async fn recently_played(api: &WebApi, token: &str, limit: u32) -> AppResult<Vec<TrackSummary>> {
    let page: Page<PlayHistoryItem> = api
        .get(
            token,
            "/me/player/recently-played",
            &[("limit", limit.min(50).to_string())],
        )
        .await?;

    let mut seen = std::collections::HashSet::new();
    Ok(page
        .items
        .into_iter()
        .map(|h| TrackSummary::from(h.track))
        .filter(|t| seen.insert(t.uri.clone()))
        .collect())
}
