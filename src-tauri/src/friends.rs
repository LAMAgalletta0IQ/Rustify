//! Event-driven Spotify friend activity.
//!
//! The public Web API has no friend-presence surface. Spotify's current
//! clients seed this view from SpClient and receive cheap per-user invalidation
//! pushes through the already-authenticated Dealer connection. A push contains
//! no activity payload; it tells us which single profile to refetch.

use std::{
    collections::HashSet,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use http::Method;
use librespot::core::session::Session;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::state::{events, AppState};

const PRESENCE_TOPIC: &str = "hm://presence2/user/";
const RESEED_INTERVAL: Duration = Duration::from_secs(30 * 60);
const CONNECTION_CHECK_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FriendFeedStatus {
    #[default]
    Connecting,
    Available,
    Empty,
    Stale,
    Unavailable,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendFeed {
    pub status: FriendFeedStatus,
    pub available: bool,
    pub entries: Vec<FriendActivity>,
    pub updated_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FriendActivity {
    pub timestamp_ms: i64,
    pub user_uri: String,
    pub user_name: String,
    pub user_image_url: Option<String>,
    pub track_uri: String,
    pub track_name: String,
    pub track_image_url: Option<String>,
    pub artist_uri: Option<String>,
    pub artist_name: Option<String>,
    pub album_uri: Option<String>,
    pub album_name: Option<String>,
    pub context_uri: Option<String>,
    pub context_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawFeed {
    friends: Option<Vec<RawEntry>>,
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    timestamp: i64,
    user: Option<RawUser>,
    track: Option<RawTrack>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUser {
    #[serde(default)]
    uri: String,
    #[serde(default)]
    name: String,
    image_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTrack {
    #[serde(default)]
    uri: String,
    #[serde(default)]
    name: String,
    image_url: Option<String>,
    album: Option<RawNamed>,
    artist: Option<RawNamed>,
    context: Option<RawNamed>,
}

#[derive(Debug, Deserialize)]
struct RawNamed {
    uri: Option<String>,
    name: Option<String>,
}

impl TryFrom<RawEntry> for FriendActivity {
    type Error = ();

    fn try_from(value: RawEntry) -> Result<Self, Self::Error> {
        let user = value.user.ok_or(())?;
        let track = value.track.ok_or(())?;
        if user.uri.trim().is_empty() || track.uri.trim().is_empty() || track.name.trim().is_empty()
        {
            return Err(());
        }
        let context_uri = track
            .context
            .as_ref()
            .and_then(|context| context.uri.clone())
            .or_else(|| track.album.as_ref().and_then(|album| album.uri.clone()));
        let context_name = track
            .context
            .as_ref()
            .and_then(|context| context.name.clone())
            .or_else(|| track.album.as_ref().and_then(|album| album.name.clone()));

        Ok(Self {
            timestamp_ms: value.timestamp,
            user_uri: user.uri,
            user_name: if user.name.trim().is_empty() {
                "Unknown".to_string()
            } else {
                user.name
            },
            user_image_url: user.image_url,
            track_uri: track.uri,
            track_name: track.name,
            track_image_url: track.image_url,
            artist_uri: track.artist.as_ref().and_then(|artist| artist.uri.clone()),
            artist_name: track.artist.as_ref().and_then(|artist| artist.name.clone()),
            album_uri: track.album.as_ref().and_then(|album| album.uri.clone()),
            album_name: track.album.as_ref().and_then(|album| album.name.clone()),
            context_uri,
            context_name,
        })
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn parse_feed(bytes: &[u8]) -> AppResult<Vec<FriendActivity>> {
    let raw: RawFeed = serde_json::from_slice(bytes)
        .map_err(|error| AppError::Other(format!("invalid friend-feed response: {error}")))?;
    let mut entries: Vec<FriendActivity> = raw
        .friends
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| entry.try_into().ok())
        .collect();
    entries.sort_by(|left, right| right.timestamp_ms.cmp(&left.timestamp_ms));
    let mut seen = HashSet::with_capacity(entries.len());
    entries.retain(|entry| seen.insert(entry.user_uri.clone()));
    Ok(entries)
}

fn parse_entry(bytes: &[u8]) -> AppResult<Option<FriendActivity>> {
    let raw: RawEntry = serde_json::from_slice(bytes)
        .map_err(|error| AppError::Other(format!("invalid friend-presence response: {error}")))?;
    Ok(raw.try_into().ok())
}

async fn seed(session: &Session, connection_id: &str) -> AppResult<Vec<FriendActivity>> {
    let endpoint = format!(
        "/presence-view/v2/init-friend-feed/{}",
        path_segment(connection_id)
    );
    let bytes = session
        .spclient()
        .request_as_json(&Method::GET, &endpoint, None, None)
        .await
        .map_err(|error| AppError::Other(format!("friend feed unavailable: {error}")))?;
    parse_feed(&bytes)
}

async fn fetch_user(session: &Session, user_id: &str) -> AppResult<Option<FriendActivity>> {
    let endpoint = format!("/presence-view/v1/user/{}", path_segment(user_id));
    let bytes = session
        .spclient()
        .request_as_json(&Method::GET, &endpoint, None, None)
        .await
        .map_err(|error| AppError::Other(format!("friend presence unavailable: {error}")))?;
    parse_entry(&bytes)
}

fn user_id_from_push(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix(PRESENCE_TOPIC)?.split(['?', '#']).next()?;
    (!value.trim().is_empty()).then_some(value.trim_matches('/'))
}

async fn emit(app: &AppHandle, feed: FriendFeed) {
    *app.state::<AppState>().friend_activity.write().await = feed.clone();
    if let Err(error) = app.emit(events::FRIENDS, feed) {
        log::warn!(target: "spotify.social", "friend activity event failed: {error}");
    }
}

async fn apply_seed(app: &AppHandle, session: &Session, connection_id: &str) -> bool {
    match seed(session, connection_id).await {
        Ok(entries) => {
            let status = if entries.is_empty() {
                FriendFeedStatus::Empty
            } else {
                FriendFeedStatus::Available
            };
            emit(
                app,
                FriendFeed {
                    status,
                    available: true,
                    entries,
                    updated_at_ms: Some(now_ms()),
                },
            )
            .await;
            true
        }
        Err(error) => {
            log::debug!(target: "spotify.social", "friend feed capability probe failed: {error}");
            let previous = app.state::<AppState>().friend_activity.read().await.clone();
            emit(
                app,
                FriendFeed {
                    status: if previous.entries.is_empty() {
                        FriendFeedStatus::Unavailable
                    } else {
                        FriendFeedStatus::Stale
                    },
                    available: false,
                    entries: previous.entries,
                    updated_at_ms: previous.updated_at_ms,
                },
            )
            .await;
            false
        }
    }
}

async fn apply_user_push(app: &AppHandle, session: &Session, user_id: &str) {
    match fetch_user(session, user_id).await {
        Ok(entry) => {
            let state = app.state::<AppState>();
            let mut feed = state.friend_activity.read().await.clone();
            feed.entries.retain(|candidate| {
                candidate.user_uri != user_id
                    && !candidate.user_uri.ends_with(&format!(":{user_id}"))
            });
            if let Some(entry) = entry {
                feed.entries.push(entry);
            }
            feed.entries
                .sort_by(|left, right| right.timestamp_ms.cmp(&left.timestamp_ms));
            feed.status = if feed.entries.is_empty() {
                FriendFeedStatus::Empty
            } else {
                FriendFeedStatus::Available
            };
            feed.available = true;
            feed.updated_at_ms = Some(now_ms());
            emit(app, feed).await;
        }
        Err(error) => log::debug!(
            target: "spotify.social",
            "friend presence refresh failed for a Dealer invalidation: {error}"
        ),
    }
}

pub fn spawn(app: AppHandle, session: Session) -> AppResult<tauri::async_runtime::JoinHandle<()>> {
    let mut updates = session
        .dealer()
        .add_listen_for(PRESENCE_TOPIC)
        .map_err(|error| AppError::Other(format!("subscribe to friend presence: {error}")))?;

    Ok(tauri::async_runtime::spawn(async move {
        let mut check = tokio::time::interval(CONNECTION_CHECK_INTERVAL);
        check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_connection_id = String::new();
        let mut next_reseed = Instant::now();

        loop {
            tokio::select! {
                _ = check.tick() => {
                    let connection_id = session.connection_id();
                    if connection_id.is_empty() {
                        continue;
                    }
                    if connection_id != last_connection_id || Instant::now() >= next_reseed {
                        let healthy = apply_seed(&app, &session, &connection_id).await;
                        last_connection_id = connection_id;
                        next_reseed = Instant::now() + if healthy {
                            RESEED_INTERVAL
                        } else {
                            Duration::from_secs(60)
                        };
                    }
                }
                update = updates.next() => match update {
                    Some(message) => {
                        if let Some(user_id) = user_id_from_push(&message.uri) {
                            apply_user_push(&app, &session, user_id).await;
                        }
                    }
                    None => {
                        let previous = app.state::<AppState>().friend_activity.read().await.clone();
                        emit(&app, FriendFeed {
                            status: FriendFeedStatus::Stale,
                            available: false,
                            entries: previous.entries,
                            updated_at_ms: previous.updated_at_ms,
                        }).await;
                        log::warn!(target: "spotify.social", "friend presence subscription ended");
                        break;
                    }
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sorts_and_deduplicates_friend_feed() {
        let json = br#"{"friends":[
          {"timestamp":100,"user":{"uri":"spotify:user:a","name":"Old"},"track":{"uri":"spotify:track:1","name":"One","artist":{"uri":"spotify:artist:x","name":"Artist"}}},
          {"timestamp":300,"user":{"uri":"spotify:user:b","name":"Bee","imageUrl":"avatar"},"track":{"uri":"spotify:track:2","name":"Two","album":{"uri":"spotify:album:y","name":"Album"}}},
          {"timestamp":200,"user":{"uri":"spotify:user:a","name":"New"},"track":{"uri":"spotify:track:3","name":"Three","context":{"uri":"spotify:playlist:z","name":"Mix"}}},
          {"timestamp":400,"user":null,"track":null}
        ]}"#;
        let parsed = parse_feed(json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].user_name, "Bee");
        assert_eq!(parsed[1].track_name, "Three");
        assert_eq!(parsed[1].context_name.as_deref(), Some("Mix"));
    }

    #[test]
    fn push_uri_and_path_segments_are_safe() {
        assert_eq!(
            user_id_from_push("hm://presence2/user/name with/slash?x=1"),
            Some("name with/slash")
        );
        assert_eq!(path_segment("a/b c"), "a%2Fb%20c");
    }
}
