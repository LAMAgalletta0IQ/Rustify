use serde::{Deserialize, Serialize};

use crate::error::AppResult;
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
            &format!("/playlists/{playlist_id}/tracks"),
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
    api.get(
        token,
        "/me/tracks/contains",
        &[("ids", ids.join(","))],
    )
    .await
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
