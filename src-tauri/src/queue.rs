//! Queue inspection and manipulation.
//!
//! Not one of the originally-listed backend modules, but queue state has no
//! clean home in `player` (librespot's Spirc exposes no queue *reader*) and it
//! is pure Web API, so it lives on its own.
//!
//! Caveat worth knowing: `GET /me/player/queue` reflects Spotify's server-side
//! queue for the *active* device. It is accurate while this app is the active
//! Connect device, which is the normal case.

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::library::TrackSummary;
use crate::webapi::WebApi;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueueView {
    pub currently_playing: Option<TrackSummary>,
    pub queue: Vec<TrackSummary>,
}

#[derive(Debug, Deserialize)]
struct WireQueue {
    currently_playing: Option<WireTrack>,
    queue: Vec<WireTrack>,
}

#[derive(Debug, Deserialize)]
struct WireImage {
    url: String,
    width: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct WireNamed {
    name: String,
}

#[derive(Debug, Deserialize)]
struct WireAlbum {
    name: String,
    images: Vec<WireImage>,
}

#[derive(Debug, Deserialize)]
struct WireTrack {
    id: Option<String>,
    uri: String,
    name: String,
    #[serde(default)]
    artists: Vec<WireNamed>,
    album: Option<WireAlbum>,
    #[serde(default)]
    duration_ms: u32,
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
            artist_ids: Vec::new(),
            album: t.album.as_ref().map(|a| a.name.clone()).unwrap_or_default(),
            image_url: t.album.as_ref().and_then(|a| pick_image(&a.images)),
            duration_ms: t.duration_ms,
            explicit: false,
        }
    }
}

pub async fn get_queue(api: &WebApi, token: &str) -> AppResult<QueueView> {
    let wire: WireQueue = api.get(token, "/me/player/queue", &[]).await?;
    Ok(QueueView {
        currently_playing: wire.currently_playing.map(Into::into),
        queue: wire.queue.into_iter().map(Into::into).collect(),
    })
}

pub async fn add_to_queue(api: &WebApi, token: &str, uri: &str) -> AppResult<()> {
    api.post(
        token,
        &format!("/me/player/queue?uri={}", urlencode(uri)),
        serde_json::Value::Null,
    )
    .await
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
