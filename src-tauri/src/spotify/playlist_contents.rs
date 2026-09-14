//! Pathfinder fallback for playlist tracklists.
//!
//! `/playlists/{id}/items` (the public Web API path `library::playlist_tracks`
//! uses) answers 404 for Spotify-*generated* playlist ids — Daily Mix,
//! Discover Weekly, Release Radar, Daylist and the rest of the personalized
//! family are not real playlist objects the public REST surface exposes to
//! third-party apps, even though they carry an ordinary `spotify:playlist:`
//! URI everywhere else (Home cards, `loadContext`, librespot's own Connect
//! playback all treat them as one).
//!
//! Since Spotify's February 2026 Web API change it also answers 403 for any
//! playlist the signed-in user does not own or collaborate on — reading a
//! public playlist that belongs to someone else, which used to be an
//! ordinary REST read, now needs a different path for every third-party app,
//! not just this one. `fetchPlaylistContents` is the same Pathfinder
//! operation the Spotify web player itself calls to render a playlist page,
//! and neither restriction applies to it — so `commands::get_playlist_tracks`
//! calls here only after the REST attempt has already come back 404 or 403,
//! rather than routing every playlist through Pathfinder by default.
//!
//! Field paths (`playlistV2.content.items[].itemV2.data...`) are undocumented
//! and parsed defensively for the same reason as `artist_extras`: a renamed
//! field degrades to a shorter list, never a hard error.

use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult};
use crate::library::TrackSummary;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistContentsPage {
    pub tracks: Vec<TrackSummary>,
    pub has_more: bool,
}

pub fn variables(playlist_id: &str, limit: u32, offset: u32) -> AppResult<Value> {
    if playlist_id.is_empty() || !playlist_id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(AppError::BadRequest(
            "invalid Spotify playlist ID".to_string(),
        ));
    }
    Ok(json!({
        "uri": format!("spotify:playlist:{playlist_id}"),
        "offset": offset,
        "limit": limit.min(100),
    }))
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn image(track: &Value) -> Option<String> {
    track
        .pointer("/albumOfTrack/coverArt/sources/0/url")
        .and_then(Value::as_str)
        .filter(|url| url.starts_with("https://"))
        .map(ToOwned::to_owned)
}

fn artists(track: &Value) -> (Vec<String>, Vec<String>) {
    track
        .pointer("/artists/items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|artist| {
                    let name = text(artist.pointer("/profile/name"))?;
                    let id = text(artist.get("uri"))
                        .and_then(|uri| uri.strip_prefix("spotify:artist:").map(ToOwned::to_owned));
                    Some((name, id))
                })
                .fold(
                    (Vec::new(), Vec::new()),
                    |(mut names, mut ids), (name, id)| {
                        names.push(name);
                        if let Some(id) = id {
                            ids.push(id);
                        }
                        (names, ids)
                    },
                )
        })
        .unwrap_or_default()
}

pub fn parse(data: &Value) -> AppResult<PlaylistContentsPage> {
    let content = data.pointer("/playlistV2/content").ok_or_else(|| {
        AppError::Unavailable("playlist contents were absent from Pathfinder".to_string())
    })?;
    let items = content
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_more = content
        .pointer("/totalCount")
        .and_then(Value::as_u64)
        .is_some_and(|total| total > items.len() as u64);

    let tracks = items
        .iter()
        .filter_map(|item| {
            let wrapper = item.get("itemV2")?;
            let type_name = text(wrapper.get("__typename"))?;
            if type_name != "TrackResponseWrapper" {
                return None;
            }
            let track = wrapper.get("data")?;
            let uri = text(track.get("uri"))?;
            let id = uri.rsplit(':').next()?.to_owned();
            let name = text(track.get("name"))?;
            let (artist_names, artist_ids) = artists(track);
            let album = text(track.pointer("/albumOfTrack/name")).unwrap_or_default();
            let duration_ms = track
                .pointer("/trackDuration/totalMilliseconds")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            Some(TrackSummary {
                id,
                uri,
                name,
                artists: artist_names,
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
        .collect();

    Ok(PlaylistContentsPage { tracks, has_more })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_playlist_id_and_bounds_limit() {
        let vars = variables("37i9dQZF1E39vTG3RmuLGB", 500, 20).unwrap();
        assert_eq!(vars["uri"], "spotify:playlist:37i9dQZF1E39vTG3RmuLGB");
        assert_eq!(vars["limit"], 100);
        assert_eq!(vars["offset"], 20);
        assert!(variables("not valid!", 10, 0).is_err());
        assert!(variables("", 10, 0).is_err());
    }

    #[test]
    fn parses_tracks_and_skips_local_and_episode_items() {
        let data = serde_json::json!({"playlistV2":{"content":{"totalCount":30,"items":[
            {"itemV2":{"__typename":"TrackResponseWrapper","data":{
                "uri":"spotify:track:0000000000000000000001","name":"Song",
                "artists":{"items":[{"uri":"spotify:artist:aaaaaaaaaaaaaaaaaaaaaa","profile":{"name":"Artist"}}]},
                "albumOfTrack":{"name":"Album","coverArt":{"sources":[{"url":"https://i.scdn.co/x"}]}},
                "trackDuration":{"totalMilliseconds":180000}
            }}},
            {"itemV2":{"__typename":"LocalTrackResponseWrapper","data":{"name":"Local file"}}},
            {"itemV2":{"__typename":"EpisodeOrChapterResponseWrapper","data":{"name":"An episode"}}}
        ]}}});
        let page = parse(&data).unwrap();
        assert_eq!(page.tracks.len(), 1);
        assert_eq!(page.tracks[0].name, "Song");
        assert_eq!(page.tracks[0].artists, vec!["Artist"]);
        assert_eq!(page.tracks[0].duration_ms, 180000);
        assert!(page.has_more);
    }

    #[test]
    fn missing_content_is_an_error_not_a_silent_empty_page() {
        assert!(parse(&serde_json::json!({})).is_err());
    }
}
