//! Parses the parts of `queryArtistOverview`'s response that `concerts.rs`
//! does not: listener stats and top tracks. Both live in the exact same
//! `data.artistUnion` payload `artist_concerts` already fetches, so folding
//! them in here costs nothing extra over the wire — it replaces what used to
//! be a REST fan-out (fetch five recent albums, then fetch every track of
//! every one of them) with fields already sitting in a response the app was
//! making anyway.
//!
//! Field paths below (`stats.monthlyListeners`, `discography.topTracks.items[].track`,
//! `coverArt.sources[]`) are not documented by Spotify — Pathfinder is
//! private — so every accessor is written defensively (`Option`, multiple
//! candidate pointers) the same way `home.rs::image` tries several image
//! paths. A wrong or renamed field degrades to an empty result rather than a
//! parse error, and callers fall back to the older REST-based top-tracks path
//! when this comes back empty (see `library::artist_tracks`).

use serde::Serialize;
use serde_json::Value;

use crate::library::TrackSummary;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArtistStats {
    pub monthly_listeners: Option<u64>,
    pub followers: Option<u64>,
}

pub fn parse_stats(data: &Value) -> ArtistStats {
    let stats = data.pointer("/artistUnion/stats");
    ArtistStats {
        monthly_listeners: stats
            .and_then(|s| s.get("monthlyListeners"))
            .and_then(Value::as_u64),
        followers: stats.and_then(|s| s.get("followers")).and_then(Value::as_u64),
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn image(value: &Value) -> Option<String> {
    ["/coverArt/sources/0/url", "/albumOfTrack/coverArt/sources/0/url"]
        .iter()
        .find_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        .filter(|url| url.starts_with("https://"))
        .map(ToOwned::to_owned)
}

fn artist_names_and_ids(track: &Value) -> (Vec<String>, Vec<String>) {
    ["/artists/items", "/firstArtist/items"]
        .iter()
        .find_map(|pointer| track.pointer(pointer).and_then(Value::as_array))
        .map(|items| {
            items
                .iter()
                .filter_map(|artist| {
                    let name = text(artist.pointer("/profile/name")).or_else(|| text(artist.get("name")))?;
                    let id = text(artist.get("uri"))
                        .and_then(|uri| uri.strip_prefix("spotify:artist:").map(ToOwned::to_owned));
                    Some((name, id))
                })
                .fold((Vec::new(), Vec::new()), |(mut names, mut ids), (name, id)| {
                    names.push(name);
                    if let Some(id) = id {
                        ids.push(id);
                    }
                    (names, ids)
                })
        })
        .unwrap_or_default()
}

/// Extracts up to 10 top tracks. Returns an empty vector — never an error —
/// when the field is absent so the caller can fall back to the REST path
/// rather than surface a Pathfinder shape change as a user-facing failure.
pub fn parse_top_tracks(data: &Value) -> Vec<TrackSummary> {
    let items = data
        .pointer("/artistUnion/discography/topTracks/items")
        .and_then(Value::as_array);
    let Some(items) = items else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.get("track"))
        .filter_map(|track| {
            let uri = text(track.get("uri"))?;
            let id = uri.rsplit(':').next()?.to_owned();
            let name = text(track.get("name"))?;
            let (artists, artist_ids) = artist_names_and_ids(track);
            let album = text(track.pointer("/albumOfTrack/name")).unwrap_or_default();
            let duration_ms = track
                .pointer("/duration/totalMilliseconds")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            Some(TrackSummary {
                id,
                uri,
                name,
                artists,
                artist_ids,
                album,
                image_url: image(track),
                duration_ms,
                explicit: track
                    .get("contentRating")
                    .and_then(|rating| rating.get("label"))
                    .and_then(Value::as_str)
                    .is_some_and(|label| label.eq_ignore_ascii_case("explicit")),
            })
        })
        .take(10)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_stats_when_present_and_missing() {
        let data = json!({"artistUnion":{"stats":{"monthlyListeners":123,"followers":456}}});
        let stats = parse_stats(&data);
        assert_eq!(stats.monthly_listeners, Some(123));
        assert_eq!(stats.followers, Some(456));
        assert_eq!(parse_stats(&json!({})), ArtistStats::default());
    }

    #[test]
    fn parses_top_tracks_with_artists_album_and_duration() {
        let data = json!({"artistUnion":{"discography":{"topTracks":{"items":[
            {"track":{
                "uri":"spotify:track:0000000000000000000001",
                "name":"Song",
                "artists":{"items":[{"uri":"spotify:artist:aaaaaaaaaaaaaaaaaaaaaa","profile":{"name":"Artist"}}]},
                "albumOfTrack":{"name":"Album","coverArt":{"sources":[{"url":"https://i.scdn.co/x"}]}},
                "duration":{"totalMilliseconds":210000},
                "contentRating":{"label":"EXPLICIT"}
            }},
            {"track":{"uri":"","name":"Broken"}}
        ]}}}});
        let tracks = parse_top_tracks(&data);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].name, "Song");
        assert_eq!(tracks[0].artists, vec!["Artist"]);
        assert_eq!(tracks[0].artist_ids, vec!["aaaaaaaaaaaaaaaaaaaaaa"]);
        assert_eq!(tracks[0].album, "Album");
        assert_eq!(tracks[0].image_url.as_deref(), Some("https://i.scdn.co/x"));
        assert_eq!(tracks[0].duration_ms, 210000);
        assert!(tracks[0].explicit);
    }

    #[test]
    fn missing_top_tracks_field_is_an_empty_result_not_an_error() {
        assert!(parse_top_tracks(&json!({"artistUnion":{}})).is_empty());
        assert!(parse_top_tracks(&json!({})).is_empty());
    }
}
