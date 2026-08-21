//! Queue inspection and manipulation.
//!
//! Not one of the originally-listed backend modules, but queue state has no
//! clean home in `player` (librespot's Spirc exposes no queue *reader*) and it
//! is pure Web API, so it lives on its own.
//!
//! Caveat worth knowing: `GET /me/player/queue` reflects Spotify's server-side
//! queue for the *active* device. It is accurate while this app is the active
//! Connect device, which is the normal case.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};

use librespot::playback::player::QueueTrack;
use librespot::protocol::player::{PlayerState, ProvidedTrack};

use crate::error::AppResult;
use crate::library::TrackSummary;
use crate::webapi::WebApi;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueueView {
    pub currently_playing: Option<TrackSummary>,
    pub previous: Vec<TrackSummary>,
    pub queue: Vec<TrackSummary>,
    pub autoplay: Vec<TrackSummary>,
    pub revision: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireQueue {
    currently_playing: Option<WireTrack>,
    #[serde(default)]
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
    #[serde(default)]
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
        previous: Vec::new(),
        queue: wire.queue.into_iter().map(Into::into).collect(),
        autoplay: Vec::new(),
        revision: None,
    })
}

/// Hydrates Dealer tracks with the richer public-API representation while
/// retaining protocol-only history, autoplay classification, and revision.
pub fn merge_metadata(protocol: &QueueView, web: QueueView) -> QueueView {
    let fallback_current = web.currently_playing.clone();
    let fallback_queue = web.queue.clone();
    let catalog: Vec<TrackSummary> = web
        .currently_playing
        .iter()
        .chain(web.queue.iter())
        .cloned()
        .collect();
    let replacement = |track: &TrackSummary| {
        catalog
            .iter()
            .find(|candidate| candidate.uri == track.uri)
            .cloned()
            .unwrap_or_else(|| track.clone())
    };
    QueueView {
        currently_playing: protocol
            .currently_playing
            .as_ref()
            .map(replacement)
            .or(fallback_current),
        previous: protocol.previous.iter().map(replacement).collect(),
        queue: if protocol.revision.is_some()
            || !protocol.previous.is_empty()
            || protocol.currently_playing.is_some()
            || !protocol.autoplay.is_empty()
        {
            protocol.queue.iter().map(replacement).collect()
        } else {
            fallback_queue
        },
        autoplay: protocol.autoplay.iter().map(replacement).collect(),
        revision: protocol.revision.clone(),
    }
}

/// Builds an immediate queue snapshot from the Connect player-state protobuf.
/// The Web API may later enrich it, but this path preserves changes delivered
/// by Dealer (including changes made by another client, a Jam, or DJ) without
/// waiting for a poll.
pub fn from_player_state(player: &PlayerState) -> QueueView {
    let mut queue = Vec::new();
    let mut autoplay = Vec::new();
    for track in player
        .next_tracks
        .iter()
        .filter(|track| valid_queue_uri(&track.uri))
        .map(provided_track)
    {
        if track.1 {
            autoplay.push(track.0);
        } else {
            queue.push(track.0);
        }
    }

    let mut currently_playing = player
        .track
        .as_ref()
        .filter(|track| valid_queue_uri(&track.uri))
        .map(provided_track)
        .map(|v| v.0);
    if let Some(current) = currently_playing.as_mut() {
        current.duration_ms = current
            .duration_ms
            .max(player.duration.clamp(0, u32::MAX as i64) as u32);
    }

    QueueView {
        currently_playing,
        previous: player
            .prev_tracks
            .iter()
            .filter(|track| valid_queue_uri(&track.uri))
            .map(provided_track)
            .map(|v| v.0)
            .collect(),
        queue,
        autoplay,
        revision: (!player.queue_revision.is_empty()).then(|| player.queue_revision.clone()),
    }
}

/// Immediate local queue projection from librespot's opt-in SetQueue event.
/// Existing metadata is retained by URI; new entries use safe placeholders
/// until the normal Web API hydration completes.
pub fn from_player_event(
    current: Option<&QueueTrack>,
    next: &[QueueTrack],
    previous: &[QueueTrack],
    existing: &QueueView,
) -> QueueView {
    let catalog: Vec<_> = existing
        .currently_playing
        .iter()
        .chain(&existing.previous)
        .chain(&existing.queue)
        .chain(&existing.autoplay)
        .collect();
    let summary = |track: &QueueTrack| {
        catalog
            .iter()
            .find(|candidate| candidate.uri == track.uri)
            .map(|track| (*track).clone())
            .unwrap_or_else(|| placeholder(&track.uri))
    };
    let mut queue = Vec::new();
    let mut autoplay = Vec::new();
    for track in next.iter().filter(|track| valid_queue_uri(&track.uri)) {
        if is_autoplay_provider(&track.provider) {
            autoplay.push(summary(track));
        } else {
            queue.push(summary(track));
        }
    }
    QueueView {
        currently_playing: current
            .filter(|track| valid_queue_uri(&track.uri))
            .map(summary),
        previous: previous
            .iter()
            .filter(|track| valid_queue_uri(&track.uri))
            .map(summary)
            .collect(),
        queue,
        autoplay,
        revision: Some(queue_revision(next)),
    }
}

fn placeholder(uri: &str) -> TrackSummary {
    TrackSummary {
        id: uri.rsplit(':').next().unwrap_or_default().to_owned(),
        uri: uri.to_owned(),
        name: "Spotify track".to_owned(),
        artists: Vec::new(),
        artist_ids: Vec::new(),
        album: String::new(),
        image_url: None,
        duration_ms: 0,
        explicit: false,
    }
}

fn queue_revision(next: &[QueueTrack]) -> String {
    let mut state = DefaultHasher::new();
    next.iter().for_each(|track| track.uri.hash(&mut state));
    state.finish().to_string()
}

fn provided_track(track: &ProvidedTrack) -> (TrackSummary, bool) {
    let metadata = &track.metadata;
    let artists: BTreeMap<usize, String> = metadata
        .iter()
        .filter_map(|(key, value)| {
            if key == "artist_name" {
                Some((0, value.clone()))
            } else {
                key.strip_prefix("artist_name:")
                    .and_then(|index| index.parse().ok())
                    .map(|index| (index, value.clone()))
            }
        })
        .collect();

    let image_url = ["image_url", "image_large_url", "image_xlarge_url"]
        .iter()
        .find_map(|key| metadata.get(*key))
        .and_then(|uri| spotify_image_url(uri));
    let id = track.uri.rsplit(':').next().unwrap_or_default().to_string();
    let provider = track.provider.to_ascii_lowercase();
    let autoplay = is_autoplay_provider(&provider)
        || metadata
            .get("autoplay.is_autoplay")
            .is_some_and(|value| value == "true");

    (
        TrackSummary {
            id,
            uri: track.uri.clone(),
            name: metadata
                .get("title")
                .cloned()
                .unwrap_or_else(|| "Spotify track".to_string()),
            artists: artists.into_values().collect(),
            artist_ids: Vec::new(),
            album: metadata.get("album_title").cloned().unwrap_or_default(),
            image_url,
            duration_ms: metadata
                .get("duration")
                .or_else(|| metadata.get("duration_ms"))
                .and_then(|value| value.parse().ok())
                .unwrap_or_default(),
            explicit: metadata
                .get("is_explicit")
                .is_some_and(|value| value == "true"),
        },
        autoplay,
    )
}

pub(crate) fn track_summary(track: &ProvidedTrack) -> TrackSummary {
    provided_track(track).0
}

fn spotify_image_url(value: &str) -> Option<String> {
    if value.starts_with("https://") {
        Some(value.to_string())
    } else {
        value
            .strip_prefix("spotify:image:")
            .filter(|id| !id.is_empty())
            .map(|id| format!("https://i.scdn.co/image/{id}"))
    }
}

pub async fn add_to_queue(api: &WebApi, token: &str, uri: &str) -> AppResult<()> {
    validate_queue_uri(uri)?;
    api.post(
        token,
        &format!("/me/player/queue?uri={}", urlencode(uri)),
        serde_json::Value::Null,
    )
    .await
}

pub fn validate_queue_uri(uri: &str) -> AppResult<()> {
    if valid_queue_uri(uri) {
        Ok(())
    } else {
        Err(crate::error::AppError::BadRequest(
            "queue items must be canonical Spotify track or episode URIs".into(),
        ))
    }
}

fn valid_queue_uri(uri: &str) -> bool {
    let mut parts = uri.split(':');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some("spotify"), Some("track" | "episode"), Some(id), None)
            if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    )
}

fn is_autoplay_provider(provider: &str) -> bool {
    provider.to_ascii_lowercase().contains("autoplay")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_queue_preserves_history_and_splits_autoplay() {
        let track = |uri: &str, provider: &str, title: &str| ProvidedTrack {
            uri: uri.to_string(),
            provider: provider.to_string(),
            metadata: [
                ("title".to_string(), title.to_string()),
                ("artist_name".to_string(), "First".to_string()),
                ("artist_name:1".to_string(), "Second".to_string()),
                ("image_url".to_string(), "spotify:image:abc".to_string()),
            ]
            .into(),
            ..Default::default()
        };
        let mut player = PlayerState {
            queue_revision: "rev-2".to_string(),
            ..Default::default()
        };
        player
            .prev_tracks
            .push(track("spotify:track:1", "context", "Previous"));
        player
            .next_tracks
            .push(track("spotify:track:2", "queue", "Queued"));
        player
            .next_tracks
            .push(track("spotify:track:3", "autoplay", "Recommended"));

        let queue = from_player_state(&player);
        assert_eq!(queue.previous[0].name, "Previous");
        assert_eq!(queue.queue[0].artists, ["First", "Second"]);
        assert_eq!(queue.autoplay[0].uri, "spotify:track:3");
        assert_eq!(queue.revision.as_deref(), Some("rev-2"));
        assert_eq!(
            queue.queue[0].image_url.as_deref(),
            Some("https://i.scdn.co/image/abc")
        );
    }

    #[test]
    fn authoritative_empty_revision_does_not_restore_stale_web_queue() {
        let protocol = QueueView {
            revision: Some("empty-revision".into()),
            ..Default::default()
        };
        let web = QueueView {
            queue: vec![placeholder("spotify:track:stale")],
            ..Default::default()
        };
        assert!(merge_metadata(&protocol, web).queue.is_empty());
    }

    #[test]
    fn local_set_queue_preserves_metadata_duplicates_and_autoplay() {
        let known = TrackSummary {
            name: "Known title".into(),
            ..placeholder("spotify:track:known")
        };
        let existing = QueueView {
            queue: vec![known],
            ..Default::default()
        };
        let next = vec![
            QueueTrack {
                uri: "spotify:track:known".into(),
                provider: "queue".into(),
            },
            QueueTrack {
                uri: "spotify:track:known".into(),
                provider: "queue".into(),
            },
            QueueTrack {
                uri: "spotify:delimiter".into(),
                provider: "context".into(),
            },
            QueueTrack {
                uri: "spotify:track:recommended".into(),
                provider: "autoplay".into(),
            },
        ];
        let queue = from_player_event(None, &next, &[], &existing);
        assert_eq!(queue.queue.len(), 2);
        assert_eq!(queue.queue[0].name, "Known title");
        assert_eq!(queue.autoplay.len(), 1);
        assert!(queue.revision.is_some());
    }

    #[test]
    fn rejects_noncanonical_queue_uris() {
        assert!(validate_queue_uri("spotify:track:abc123").is_ok());
        assert!(validate_queue_uri("spotify:episode:abc123").is_ok());
        assert!(validate_queue_uri("spotify:playlist:abc123").is_err());
        assert!(validate_queue_uri("spotify:track:abc/../def").is_err());
        assert!(validate_queue_uri("https://open.spotify.com/track/abc").is_err());
    }
}
