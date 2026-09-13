//! Event-driven Spotify friend activity.
//!
//! The public Web API has no friend-presence surface. Spotify's current
//! clients seed this view from SpClient and receive cheap per-user invalidation
//! pushes through the already-authenticated Dealer connection. A push contains
//! no activity payload; it tells us which single profile to refetch.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use futures_util::{stream::FuturesUnordered, StreamExt};
use http::Method;
use librespot::core::{session::Session, SpotifyUri};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::dealer_util::is_builder_not_available;
use crate::error::{AppError, AppResult};
use crate::state::{events, AppState};

const PRESENCE_TOPIC: &str = "hm://presence2/user/";
const RESEED_INTERVAL: Duration = Duration::from_secs(30 * 60);
const CONNECTION_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const FAILED_RESEED_INTERVAL: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 500;
const MAX_FIELD_CHARS: usize = 2_048;
const MAX_USER_ID_BYTES: usize = 256;
const MAX_CONCURRENT_REFRESHES: usize = 64;

type RefreshFuture = Pin<
    Box<dyn Future<Output = (String, u64, AppResult<Option<FriendActivity>>)> + Send + 'static>,
>;

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FriendFeedStatus {
    #[default]
    Connecting,
    Available,
    Empty,
    /// A refresh just failed but there is cached activity to keep showing —
    /// regardless of *why* it failed, since something is still displayable.
    Stale,
    /// Spotify answered 403/404: this account or region genuinely does not
    /// have the capability. Only ever set from that specific evidence.
    Unavailable,
    /// Any other failure with nothing cached yet — expired token, rate
    /// limit, server error, dropped Dealer connection, unexpected response
    /// shape. Distinct from `Unavailable` on purpose: this is expected to
    /// recover on its own and must not be worded as a capability limit.
    Failed,
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
        if !user.uri.starts_with("spotify:user:")
            || !matches!(
                SpotifyUri::from_uri(&track.uri),
                Ok(SpotifyUri::Track { .. })
            )
            || track.name.trim().is_empty()
        {
            return Err(());
        }
        let context_uri = track
            .context
            .as_ref()
            .and_then(|context| safe_spotify_uri(context.uri.as_deref()))
            .or_else(|| {
                track
                    .album
                    .as_ref()
                    .and_then(|album| safe_spotify_uri(album.uri.as_deref()))
            });
        let context_name = track
            .context
            .as_ref()
            .and_then(|context| context.name.clone())
            .or_else(|| track.album.as_ref().and_then(|album| album.name.clone()));

        Ok(Self {
            timestamp_ms: value.timestamp.max(0),
            user_uri: bounded(&user.uri),
            user_name: if user.name.trim().is_empty() {
                "Unknown".to_string()
            } else {
                bounded(&user.name)
            },
            user_image_url: safe_image_url(user.image_url),
            track_uri: track.uri,
            track_name: bounded(&track.name),
            track_image_url: safe_image_url(track.image_url),
            artist_uri: track
                .artist
                .as_ref()
                .and_then(|artist| safe_spotify_uri(artist.uri.as_deref())),
            artist_name: track
                .artist
                .as_ref()
                .and_then(|artist| artist.name.as_deref().map(bounded)),
            album_uri: track
                .album
                .as_ref()
                .and_then(|album| safe_spotify_uri(album.uri.as_deref())),
            album_name: track
                .album
                .as_ref()
                .and_then(|album| album.name.as_deref().map(bounded)),
            context_uri,
            context_name: context_name.map(|name| bounded(&name)),
        })
    }
}

fn bounded(value: &str) -> String {
    value.trim().chars().take(MAX_FIELD_CHARS).collect()
}

fn safe_spotify_uri(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (value.starts_with("spotify:") && SpotifyUri::from_uri(value).is_ok())
        .then(|| value.to_string())
}

fn safe_image_url(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_string();
    reqwest::Url::parse(&value)
        .ok()
        .filter(|url| url.scheme() == "https" && url.host_str().is_some())
        .map(|_| value)
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
    ensure_response_bound(bytes)?;
    let raw: RawFeed = serde_json::from_slice(bytes)
        .map_err(|error| AppError::Other(format!("invalid friend-feed response: {error}")))?;
    let mut entries: Vec<FriendActivity> = raw
        .friends
        .unwrap_or_default()
        .into_iter()
        .take(MAX_ENTRIES)
        .filter_map(|entry| entry.try_into().ok())
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
    let mut seen = HashSet::with_capacity(entries.len());
    entries.retain(|entry| seen.insert(entry.user_uri.clone()));
    Ok(entries)
}

fn parse_entry(bytes: &[u8]) -> AppResult<Option<FriendActivity>> {
    ensure_response_bound(bytes)?;
    let raw: RawEntry = serde_json::from_slice(bytes)
        .map_err(|error| AppError::Other(format!("invalid friend-presence response: {error}")))?;
    Ok(raw.try_into().ok())
}

fn ensure_response_bound(bytes: &[u8]) -> AppResult<()> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(AppError::Other(
            "friend presence response exceeded 2 MiB".to_string(),
        ));
    }
    Ok(())
}

/// librespot's SpClient maps HTTP status onto a generic `Error { kind, .. }`
/// (see `librespot_core::http_client`'s `HttpClientError -> Error` impl —
/// 404/410 -> NotFound, 403/402 -> PermissionDenied, 401 -> Unauthenticated,
/// 429 -> ResourceExhausted, 5xx -> Unavailable). The blanket
/// `From<librespot::core::Error> for AppError` throws that away into one
/// `Playback` variant by design (its own doc comment: "callers pick the
/// variant") — friends.rs needs the distinction to avoid telling a user
/// their account/region lacks a feature when the real cause was an
/// expired token, a rate limit, or a dropped connection.
fn classify_spclient_error(context: &str, error: librespot::core::Error) -> AppError {
    use librespot::core::error::ErrorKind;
    match error.kind {
        ErrorKind::NotFound => AppError::Unavailable(format!("{context}: {error}")),
        ErrorKind::PermissionDenied => AppError::Forbidden(format!("{context}: {error}")),
        ErrorKind::Unauthenticated => AppError::SessionExpired,
        ErrorKind::ResourceExhausted => AppError::RateLimited { retry_after: None },
        ErrorKind::Unavailable | ErrorKind::DeadlineExceeded => {
            AppError::ServiceUnavailable { status: 503 }
        }
        _ => AppError::Other(format!("{context}: {error}")),
    }
}

/// True only for the specific failures that mean "this account or region
/// does not have this capability" — a 404 (endpoint doesn't exist for this
/// client/account) or 403 (exists, refused). Everything else — expired
/// token, rate limit, server hiccup, a dropped Dealer connection, a JSON
/// shape change — is transient and must not be reported as a capability
/// limitation.
fn is_capability_absent(error: &AppError) -> bool {
    matches!(error, AppError::Unavailable(_) | AppError::Forbidden(_))
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
        .map_err(|error| classify_spclient_error("friend feed", error))?;
    parse_feed(&bytes)
}

async fn fetch_user(session: &Session, user_id: &str) -> AppResult<Option<FriendActivity>> {
    let endpoint = format!("/presence-view/v1/user/{}", path_segment(user_id));
    let bytes = session
        .spclient()
        .request_as_json(&Method::GET, &endpoint, None, None)
        .await
        .map_err(|error| classify_spclient_error("friend presence", error))?;
    parse_entry(&bytes)
}

fn user_id_from_push(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix(PRESENCE_TOPIC)?.split(['?', '#']).next()?;
    let value = value.trim_matches('/');
    (!value.is_empty() && value.len() <= MAX_USER_ID_BYTES && !value.chars().any(char::is_control))
        .then_some(value)
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
            if session.connection_id() != connection_id {
                log::debug!(target: "spotify.social", "dropping friend seed from superseded Dealer connection");
                return false;
            }
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
            let capability_absent = is_capability_absent(&error);
            log::debug!(
                target: "spotify.social",
                "friend feed seed failed (capability_absent={capability_absent}): {error}"
            );
            let previous = app.state::<AppState>().friend_activity.read().await.clone();
            emit(
                app,
                FriendFeed {
                    status: if !previous.entries.is_empty() {
                        FriendFeedStatus::Stale
                    } else if capability_absent {
                        FriendFeedStatus::Unavailable
                    } else {
                        FriendFeedStatus::Failed
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

async fn apply_user_result(
    app: &AppHandle,
    user_id: &str,
    result: AppResult<Option<FriendActivity>>,
) {
    match result {
        Ok(entry) => {
            let state = app.state::<AppState>();
            let mut feed = state.friend_activity.read().await.clone();
            if merge_user_result(&mut feed, user_id, entry) {
                emit(app, feed).await;
            }
        }
        Err(error) => log::debug!(
            target: "spotify.social",
            "friend presence refresh failed for a Dealer invalidation: {error}"
        ),
    }
}

fn merge_user_result(feed: &mut FriendFeed, user_id: &str, entry: Option<FriendActivity>) -> bool {
    if entry.as_ref().is_some_and(|entry| {
        feed.entries.iter().any(|candidate| {
            candidate.user_uri.eq_ignore_ascii_case(&entry.user_uri)
                && candidate.timestamp_ms > entry.timestamp_ms
        })
    }) {
        return false;
    }
    feed.entries
        .retain(|candidate| !activity_matches_user(candidate, user_id));
    if let Some(entry) = entry {
        feed.entries.push(entry);
    }
    feed.entries
        .sort_by_key(|entry| std::cmp::Reverse(entry.timestamp_ms));
    feed.status = if feed.entries.is_empty() {
        FriendFeedStatus::Empty
    } else {
        FriendFeedStatus::Available
    };
    feed.available = true;
    feed.updated_at_ms = Some(now_ms());
    true
}

fn activity_matches_user(activity: &FriendActivity, user_id: &str) -> bool {
    activity.user_uri.eq_ignore_ascii_case(user_id)
        || activity
            .user_uri
            .rsplit(':')
            .next()
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(user_id))
}

fn enqueue_refresh(
    refreshes: &mut FuturesUnordered<RefreshFuture>,
    session: &Session,
    user_id: String,
    generation: u64,
) {
    let session = session.clone();
    refreshes.push(Box::pin(async move {
        let result = fetch_user(&session, &user_id).await;
        (user_id, generation, result)
    }));
}

pub fn spawn(app: AppHandle, session: Session) -> AppResult<tauri::async_runtime::JoinHandle<()>> {
    Ok(tauri::async_runtime::spawn(async move {
        let mut updates = {
            let mut delay = Duration::from_millis(200);
            let mut other_error_delay = Duration::from_secs(1);
            loop {
                match session.dealer().add_listen_for(PRESENCE_TOPIC) {
                    Ok(subscription) => break subscription,
                    Err(error) if is_builder_not_available(&error) => {
                        log::debug!(
                            target: "spotify.social",
                            "dealer not yet ready, retrying friend presence subscription in {delay:?}: {error}"
                        );
                        tokio::time::sleep(delay).await;
                        delay = (delay * 2).min(Duration::from_secs(2));
                        continue;
                    }
                    Err(error) => {
                        // Never give up outright — friend presence is an optional
                        // Tier 2 feature that degrades, per CLAUDE.md, rather than
                        // blocking anything else. Back off the same way the
                        // transient-race branch above does, capped higher, so a
                        // persistently broken dealer doesn't retry at a fixed 1/s
                        // forever with no escalation.
                        log::warn!(
                            target: "spotify.social",
                            "subscribe to friend presence failed: {error}"
                        );
                        tokio::time::sleep(other_error_delay).await;
                        other_error_delay = (other_error_delay * 2).min(Duration::from_secs(30));
                        continue;
                    }
                }
            }
        };
        let mut check = tokio::time::interval(CONNECTION_CHECK_INTERVAL);
        check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_connection_id = String::new();
        let mut next_reseed = Instant::now();
        let mut generation = 0_u64;
        let mut updates_open = true;
        let mut refreshes = FuturesUnordered::<RefreshFuture>::new();
        let mut in_flight = HashMap::<String, u64>::new();
        let mut dirty = HashSet::<String>::new();

        loop {
            tokio::select! {
                _ = check.tick() => {
                    let connection_id = session.connection_id();
                    if connection_id.is_empty() {
                        if !last_connection_id.is_empty() {
                            generation = generation.wrapping_add(1);
                            in_flight.clear();
                            dirty.clear();
                            let previous = app.state::<AppState>().friend_activity.read().await.clone();
                            emit(&app, FriendFeed {
                                // Losing the Dealer connection id is never evidence of a
                                // missing capability — only a 403/404 from Spotify itself is.
                                status: if previous.entries.is_empty() { FriendFeedStatus::Failed } else { FriendFeedStatus::Stale },
                                available: false,
                                entries: previous.entries,
                                updated_at_ms: previous.updated_at_ms,
                            }).await;
                            last_connection_id.clear();
                        }
                        continue;
                    }
                    if connection_id != last_connection_id || Instant::now() >= next_reseed {
                        generation = generation.wrapping_add(1);
                        in_flight.clear();
                        dirty.clear();
                        let healthy = apply_seed(&app, &session, &connection_id).await;
                        last_connection_id = connection_id;
                        next_reseed = Instant::now() + if healthy {
                            RESEED_INTERVAL
                        } else {
                            FAILED_RESEED_INTERVAL
                        };
                    }
                },
                update = updates.next(), if updates_open => match update {
                    Some(message) => {
                        if let Some(user_id) = user_id_from_push(&message.uri) {
                            let user_id = user_id.to_string();
                            if in_flight.get(&user_id) != Some(&generation) {
                                if in_flight.len() >= MAX_CONCURRENT_REFRESHES {
                                    // A seed is cheaper and bounded compared with
                                    // retaining an arbitrary number of per-user futures.
                                    next_reseed = Instant::now();
                                    continue;
                                }
                                in_flight.insert(user_id.clone(), generation);
                                enqueue_refresh(&mut refreshes, &session, user_id, generation);
                            } else {
                                dirty.insert(user_id);
                            }
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
                        updates_open = false;
                        next_reseed = Instant::now() + FAILED_RESEED_INTERVAL;
                    }
                },
                result = refreshes.next(), if !refreshes.is_empty() => {
                    if let Some((user_id, result_generation, result)) = result {
                        if in_flight.get(&user_id) != Some(&result_generation) {
                            continue;
                        }
                        in_flight.remove(&user_id);
                        if result_generation == generation {
                            apply_user_result(&app, &user_id, result).await;
                        }
                        if dirty.remove(&user_id) {
                            in_flight.insert(user_id.clone(), generation);
                            enqueue_refresh(&mut refreshes, &session, user_id, generation);
                        }
                    }
                },
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use librespot::core::error::{Error as CoreError, ErrorKind};

    #[test]
    fn only_403_and_404_classify_as_capability_absent() {
        let cases = [
            (CoreError::not_found("x"), true),
            (CoreError::permission_denied("x"), true),
            (CoreError::unauthenticated("x"), false),
            (CoreError::resource_exhausted("x"), false),
            (CoreError::unavailable("x"), false),
            (CoreError::new(ErrorKind::Internal, "x"), false),
        ];
        for (error, expect_absent) in cases {
            let kind = error.kind;
            let mapped = classify_spclient_error("ctx", error);
            assert_eq!(
                is_capability_absent(&mapped),
                expect_absent,
                "kind {kind:?} mapped to {mapped:?}, capability_absent should be {expect_absent}"
            );
        }
    }

    #[test]
    fn expired_token_and_rate_limit_map_to_their_own_app_error_variants() {
        assert!(matches!(
            classify_spclient_error("ctx", CoreError::unauthenticated("x")),
            AppError::SessionExpired
        ));
        assert!(matches!(
            classify_spclient_error("ctx", CoreError::resource_exhausted("x")),
            AppError::RateLimited { retry_after: None }
        ));
    }

    #[test]
    fn parses_sorts_and_deduplicates_friend_feed() {
        let json = br#"{"friends":[
          {"timestamp":100,"user":{"uri":"spotify:user:a","name":"Old"},"track":{"uri":"spotify:track:0000000000000000000001","name":"One","artist":{"uri":"spotify:artist:0000000000000000000001","name":"Artist"}}},
          {"timestamp":300,"user":{"uri":"spotify:user:b","name":"Bee","imageUrl":"https://i.scdn.co/avatar"},"track":{"uri":"spotify:track:0000000000000000000002","name":"Two","album":{"uri":"spotify:album:0000000000000000000002","name":"Album"}}},
          {"timestamp":200,"user":{"uri":"spotify:user:a","name":"New"},"track":{"uri":"spotify:track:0000000000000000000003","name":"Three","context":{"uri":"spotify:playlist:0000000000000000000003","name":"Mix"}}},
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
        let oversized = format!("{PRESENCE_TOPIC}{}", "x".repeat(MAX_USER_ID_BYTES + 1));
        assert_eq!(user_id_from_push(&oversized), None);
    }

    #[test]
    fn rejects_unsafe_media_and_non_track_activity() {
        let json = br#"{"friends":[
          {"timestamp":100,"user":{"uri":"spotify:user:a","name":"A","imageUrl":"http://unsafe/avatar"},"track":{"uri":"spotify:track:0000000000000000000000","name":"Track","imageUrl":"javascript:bad","context":{"uri":"https://bad","name":"Bad"}}},
          {"timestamp":200,"user":{"uri":"spotify:user:b","name":"B"},"track":{"uri":"spotify:episode:0000000000000000000000","name":"Episode"}}
        ]}"#;
        let parsed = parse_feed(json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].user_image_url, None);
        assert_eq!(parsed[0].track_image_url, None);
        assert_eq!(parsed[0].context_uri, None);
    }

    #[test]
    fn stale_user_refresh_cannot_replace_newer_seed_activity() {
        let newer = FriendActivity {
            timestamp_ms: 300,
            user_uri: "spotify:user:alice".into(),
            user_name: "Alice".into(),
            user_image_url: None,
            track_uri: "spotify:track:0000000000000000000003".into(),
            track_name: "New".into(),
            track_image_url: None,
            artist_uri: None,
            artist_name: None,
            album_uri: None,
            album_name: None,
            context_uri: None,
            context_name: None,
        };
        let mut feed = FriendFeed {
            entries: vec![newer.clone()],
            ..Default::default()
        };
        let mut older = newer;
        older.timestamp_ms = 200;
        older.track_name = "Old".into();
        assert!(!merge_user_result(&mut feed, "alice", Some(older)));
        assert_eq!(feed.entries[0].track_name, "New");
        assert!(merge_user_result(&mut feed, "ALICE", None));
        assert!(feed.entries.is_empty());
        assert_eq!(feed.status, FriendFeedStatus::Empty);
    }
}
