use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::library::{AlbumSummary, TrackSummary};
use crate::webapi::WebApi;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistSummary {
    pub uri: String,
    pub id: String,
    pub name: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub tracks: Vec<TrackSummary>,
    pub albums: Vec<AlbumSummary>,
    pub artists: Vec<ArtistSummary>,
    pub playlists: Vec<PlaylistHit>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistHit {
    pub uri: String,
    pub id: String,
    pub name: String,
    pub owner: String,
    pub image_url: Option<String>,
}

// ---- wire types ---------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearchResponse {
    tracks: Option<Wrap<WireTrack>>,
    albums: Option<Wrap<WireAlbum>>,
    artists: Option<Wrap<WireArtist>>,
    playlists: Option<Wrap<WirePlaylist>>,
}

#[derive(Debug, Deserialize)]
struct Wrap<T> {
    items: Vec<Option<T>>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireImage {
    url: String,
    width: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct WireNamed {
    #[serde(default)]
    id: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct WireOwner {
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireAlbum {
    id: String,
    uri: String,
    name: String,
    artists: Vec<WireNamed>,
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
    artists: Vec<WireNamed>,
    album: Option<WireAlbum>,
    duration_ms: u32,
    #[serde(default)]
    explicit: bool,
}

#[derive(Debug, Deserialize)]
struct WireArtist {
    id: String,
    uri: String,
    name: String,
    images: Option<Vec<WireImage>>,
}

#[derive(Debug, Deserialize)]
struct WirePlaylist {
    id: String,
    uri: String,
    name: String,
    owner: WireOwner,
    images: Option<Vec<WireImage>>,
}

fn pick_image(images: &[WireImage]) -> Option<String> {
    images
        .iter()
        .min_by_key(|i| (i.width.unwrap_or(640) as i32 - 300).abs())
        .map(|i| i.url.clone())
}

/// Spotify's ceiling on `limit` for `/search`, per item type.
///
/// Deliberately not the 50 used elsewhere: the search endpoint is stricter than
/// the library ones and rejects anything higher outright.
pub const MAX_SEARCH_LIMIT: u32 = 10;

/// Search across tracks, albums, artists and playlists in one request.
///
/// Note: Spotify returns `null` entries inside `items` for unavailable
/// results, hence `Vec<Option<T>>` and the `flatten`.
pub async fn search(
    api: &WebApi,
    token: &str,
    query: &str,
    limit: u32,
    offset: u32,
) -> AppResult<SearchResults> {
    if query.trim().is_empty() {
        return Ok(SearchResults::default());
    }

    let resp: SearchResponse = api
        .get(
            token,
            "/search",
            &[
                ("q", query.to_string()),
                ("type", "track,album,artist,playlist".to_string()),
                // `/search` caps `limit` at 10 — far lower than the 50 most
                // other endpoints allow. Exceeding it is a 400 "Invalid limit",
                // not a silent truncation, so this clamp is load-bearing.
                ("limit", limit.clamp(1, MAX_SEARCH_LIMIT).to_string()),
                ("offset", offset.to_string()),
            ],
        )
        .await?;

    let has_more = resp.tracks.as_ref().is_some_and(|w| w.next.is_some())
        || resp.albums.as_ref().is_some_and(|w| w.next.is_some())
        || resp.artists.as_ref().is_some_and(|w| w.next.is_some())
        || resp.playlists.as_ref().is_some_and(|w| w.next.is_some());
    Ok(SearchResults {
        has_more,
        tracks: resp
            .tracks
            .map(|w| {
                w.items
                    .into_iter()
                    .flatten()
                    .map(|t| TrackSummary {
                        artist_ids: t.artists.iter().filter_map(|a| a.id.clone()).collect(),
                        id: t.id.unwrap_or_default(),
                        uri: t.uri,
                        name: t.name,
                        artists: t.artists.into_iter().map(|a| a.name).collect(),
                        album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
                        image_url: t.album.as_ref().and_then(|a| pick_image(&a.images)),
                        duration_ms: t.duration_ms,
                        explicit: t.explicit,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        albums: resp
            .albums
            .map(|w| {
                w.items
                    .into_iter()
                    .flatten()
                    .map(|a| AlbumSummary {
                        image_url: pick_image(&a.images),
                        id: a.id,
                        uri: a.uri,
                        name: a.name,
                        artists: a.artists.into_iter().map(|x| x.name).collect(),
                        album_type: a.album_type,
                        release_date: a.release_date,
                        release_date_precision: a.release_date_precision,
                        total_tracks: a.total_tracks,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        artists: resp
            .artists
            .map(|w| {
                w.items
                    .into_iter()
                    .flatten()
                    .map(|a| ArtistSummary {
                        image_url: a.images.as_deref().and_then(pick_image),
                        id: a.id,
                        uri: a.uri,
                        name: a.name,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        playlists: resp
            .playlists
            .map(|w| {
                w.items
                    .into_iter()
                    .flatten()
                    .map(|p| PlaylistHit {
                        image_url: p.images.as_deref().and_then(pick_image),
                        owner: p.owner.display_name.unwrap_or_default(),
                        id: p.id,
                        uri: p.uri,
                        name: p.name,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}
