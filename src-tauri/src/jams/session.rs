use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use super::client_token::ClientTokenManager;
use super::config::JamConfig;
use super::dealer::{DealerClient, JamEvent};
use super::error::JamError;
use super::pathfinder::PathfinderClient;
use super::spclient::JamApiClient;
use super::token::{AccessToken, TokenProvider};
use super::{ClientIdentity, ConnectionId};

/// A member of a social-connect session. This is the union of librespot's
/// `social_connect_v2.proto` and the additional fields returned by the v3 HTTP
/// session endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct JamMember {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_host: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub large_image_url: Option<String>,
    #[serde(default)]
    pub joined_timestamp: Option<i64>,
    #[serde(default)]
    pub is_listening: bool,
    #[serde(default)]
    pub is_controlling: bool,
    #[serde(default)]
    pub playback_control: Option<String>,
    #[serde(default)]
    pub is_current_user: bool,
}

/// A track in a jam queue, decoded best-effort.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct JamTrack {
    pub uri: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub artists: Vec<String>,
    #[serde(default)]
    pub added_by: Option<String>,
    #[serde(default)]
    pub added_at: Option<String>,
}

/// A jam and its current state. Parsed defensively: fields the response omits
/// fall back to their defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct JamSession {
    pub id: String,
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub host: Option<JamMember>,
    #[serde(default)]
    pub members: Vec<JamMember>,
    #[serde(default)]
    pub queue: Vec<JamTrack>,
    #[serde(default)]
    pub active_state: Value,
    /// Token from the invite link; what [`JamManager::join`] takes.
    #[serde(default)]
    pub join_token: Option<String>,
    /// Shareable invite link, `https://open.spotify.com/socialsession/<token>`.
    #[serde(default)]
    pub join_url: Option<String>,
    /// Spotify URI form of the invite (`spotify:socialsession:<token>`).
    #[serde(default)]
    pub join_uri: Option<String>,
    #[serde(default)]
    pub is_session_owner: bool,
    #[serde(default)]
    pub is_listening: bool,
    #[serde(default)]
    pub is_controlling: bool,
    #[serde(default)]
    pub is_discoverable: bool,
    #[serde(default)]
    pub session_type: Option<String>,
    #[serde(default)]
    pub host_active_device_id: Option<String>,
    #[serde(default)]
    pub max_member_count: Option<u32>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub queue_only_mode: bool,
    #[serde(default = "default_true")]
    pub queue_control_allowed: bool,
    #[serde(default)]
    pub wifi_broadcast: bool,
    #[serde(default)]
    pub host_device_info: Option<Value>,
}

fn default_true() -> bool {
    true
}

/// Everything the module needs to authenticate, gathered in one place because
/// three of the four values have to come from the *same* live session to agree
/// with each other.
///
/// Spotify ties a jam to a device (`identity.device_id`) and pushes its updates
/// over a dealer connection (`connection_id`); if those two belong to different
/// clients, or the `client-token` was minted for a different one, the request
/// is refused with a bare 400 and no explanation.
pub struct JamCredentials {
    /// Who this client claims to be. `device_id` must be the live Connect id.
    pub identity: ClientIdentity,
    /// Bearer for the **first-party** services (spclient, Pathfinder). Must be
    /// a token issued under a Spotify client id — Rustify passes librespot's
    /// keymaster token. A token from a self-registered app is refused with
    /// `403 RBAC: access denied` regardless of scope.
    pub access_token: Arc<dyn TokenProvider>,
    /// Bearer for public `api.spotify.com` calls (the queue endpoint behind
    /// `add_track`). `None` reuses `access_token`.
    pub web_api_token: Option<Arc<dyn TokenProvider>>,
    /// Cell the host keeps filled with a `client-token` it obtained itself
    /// (Rustify: librespot's). `None` falls back to minting one here, which
    /// standalone use needs and the app does not — see
    /// [`ClientTokenManager::supplied`].
    pub client_token: Option<AccessToken>,
    /// Server-assigned `Spotify-Connection-Id`; the host writes librespot's
    /// into it. Empty means the header is omitted.
    pub connection_id: ConnectionId,
}

impl JamCredentials {
    /// Standalone credentials: mint a client token, no connection id, random
    /// device id. Enough to exercise the module; not enough to create a jam
    /// that controls real playback.
    pub fn standalone(access_token: Arc<dyn TokenProvider>) -> Self {
        Self {
            identity: ClientIdentity::default(),
            access_token,
            web_api_token: None,
            client_token: None,
            connection_id: ConnectionId::new(),
        }
    }
}

/// High-level entry point combining SpClient actions, Pathfinder metadata and
/// the dealer event stream.
///
/// Construct once with [`JamManager::new`]; the dealer listener is spawned
/// immediately and survives for the life of the manager (aborted on drop).
/// The type is `Send + Sync`, so it can be shared behind an `Arc` and its
/// methods awaited from multiple tasks.
pub struct JamManager {
    /// Kept for unauthenticated side calls — resolving an invite shortlink is
    /// a plain redirect follow, not an API request.
    http: reqwest::Client,
    api: Arc<JamApiClient>,
    pathfinder: PathfinderClient,
    dealer_task: Option<JoinHandle<()>>,
    /// `Option` so the receiver can be handed out exactly once; `Mutex` so the
    /// manager stays `Sync` (a bare `Receiver` is not) while it sits behind
    /// the app's `Arc`.
    events: std::sync::Mutex<Option<mpsc::Receiver<JamEvent>>>,
}

impl JamManager {
    /// Builds every client from a single `reqwest::Client` and config, then
    /// spawns the dealer listener.
    pub fn new(
        http: reqwest::Client,
        config: JamConfig,
        credentials: JamCredentials,
    ) -> Result<Self, JamError> {
        Self::build(http, config, credentials, true)
    }

    /// Builds the HTTP clients without opening another Dealer websocket.
    /// Host applications that already run librespot must use this and consume
    /// social-connect updates from librespot's authenticated Dealer manager.
    pub fn new_http_only(
        http: reqwest::Client,
        config: JamConfig,
        credentials: JamCredentials,
    ) -> Result<Self, JamError> {
        Self::build(http, config, credentials, false)
    }

    fn build(
        http: reqwest::Client,
        config: JamConfig,
        credentials: JamCredentials,
        start_dealer: bool,
    ) -> Result<Self, JamError> {
        let JamCredentials {
            identity,
            access_token,
            web_api_token,
            client_token: supplied,
            connection_id,
        } = credentials;

        let client_token = match supplied {
            Some(cell) => ClientTokenManager::supplied(http.clone(), identity.clone(), cell),
            None => ClientTokenManager::with_identity(
                http.clone(),
                config.client_token_url.clone(),
                identity.clone(),
            ),
        };
        let api = Arc::new(JamApiClient::new(
            http.clone(),
            &config,
            &identity,
            connection_id.clone(),
            access_token.clone(),
            web_api_token,
            client_token.clone(),
        )?);
        let pathfinder = PathfinderClient::new(
            http.clone(),
            &config,
            connection_id.clone(),
            access_token.clone(),
            client_token.clone(),
        );
        let (dealer_task, events) = if start_dealer {
            let (dealer, events) = DealerClient::new(
                config.dealer_url.clone(),
                connection_id,
                access_token,
                client_token,
                256,
            );
            (Some(dealer.spawn()), events)
        } else {
            let (_tx, events) = mpsc::channel(1);
            (None, events)
        };
        Ok(Self {
            http,
            api,
            pathfinder,
            dealer_task,
            events: std::sync::Mutex::new(Some(events)),
        })
    }

    /// Raw SpClient client, for ad-hoc probing before endpoints are captured.
    pub fn api(&self) -> &JamApiClient {
        &self.api
    }

    /// Runs a registered Pathfinder persisted query by operation name.
    pub async fn pathfinder_query(
        &self,
        operation_name: &str,
        variables: Value,
    ) -> Result<Value, JamError> {
        self.pathfinder
            .query_registered(operation_name, variables)
            .await
    }

    pub async fn create(&self, context_uri: Option<&str>) -> Result<JamSession, JamError> {
        let resp = self.api.create_jam(context_uri).await?;
        self.hydrate(resp).await
    }

    pub async fn current(&self) -> Result<JamSession, JamError> {
        let resp = self.api.current_jam().await?;
        self.hydrate(resp).await
    }

    /// Joins by invite link, `spotify:socialsession:` URI, or bare join token.
    pub async fn join(&self, jam_id: &str) -> Result<JamSession, JamError> {
        let token = self.resolve_join_token(jam_id).await?;
        let resp = self.api.join_jam(&token).await?;
        self.hydrate(resp).await
    }

    /// Reduces whatever the user pasted to a join session token, following a
    /// shortlink first if that is what they pasted.
    ///
    /// The share sheet hands out `https://spotify.link/<id>`, whose last path
    /// segment is the **shortlink id** — 11 characters, and nothing to do with
    /// the session. Sending it earns `400 BAD_JOIN_TOKEN`. The join token
    /// exists only in the URL the shortlink redirects to, so the only way to
    /// get it is to follow the redirect.
    async fn resolve_join_token(&self, input: &str) -> Result<String, JamError> {
        let input = input.trim();
        let is_url = input.starts_with("http://") || input.starts_with("https://");
        if !is_url || input.contains("socialsession") {
            return Ok(join_token(input).to_owned());
        }
        debug!("following {input} to find the join token behind it");
        // No auth headers: this is a public redirect, and reqwest follows it
        // for us — the answer is the URL the response ended up at.
        let resolved = self.http.get(input).send().await?.url().to_string();
        if !resolved.contains("socialsession") {
            warn!("{input} resolved to {resolved}, which carries no join token");
        }
        Ok(join_token(&resolved).to_owned())
    }

    pub async fn add_track(&self, jam_id: &str, track_uri: &str) -> Result<(), JamError> {
        self.api.add_track(jam_id, track_uri).await?;
        Ok(())
    }

    pub async fn reorder_queue(&self, jam_id: &str, new_order: Vec<usize>) -> Result<(), JamError> {
        self.api.reorder_queue(jam_id, new_order).await?;
        Ok(())
    }

    pub async fn leave(&self, jam_id: &str) -> Result<(), JamError> {
        self.api.leave(jam_id).await?;
        Ok(())
    }

    pub async fn set_queue_control(&self, allowed: bool) -> Result<JamSession, JamError> {
        self.api.set_queue_control(allowed).await?;
        self.current().await
    }

    pub async fn kick(&self, jam_id: &str, member_id: &str) -> Result<JamSession, JamError> {
        let response = self.api.kick_member(jam_id, member_id).await?;
        self.hydrate(response).await
    }

    pub async fn end(&self, jam_id: &str) -> Result<(), JamError> {
        self.api.end_jam(jam_id).await?;
        Ok(())
    }

    /// Returns the dealer event stream. Ownership transfers to the caller, so
    /// call this exactly once and pass the receiver onward. Returns `None` on
    /// a second call and on a poisoned mutex, rather than panicking — this
    /// runs inside a Tauri app, where a panic on a background thread takes
    /// the whole process down with no visible error.
    pub fn events(&self) -> Option<mpsc::Receiver<JamEvent>> {
        self.events.lock().ok()?.take()
    }

    /// Enriches a raw jam payload with member metadata from Pathfinder when a
    /// `FetchJam` persisted query is registered; otherwise returns the raw
    /// shape unchanged. Pathfinder failure is logged, not fatal — the SpClient
    /// response already carries the jam id.
    async fn hydrate(&self, raw: Value) -> Result<JamSession, JamError> {
        let mut session = session_from_value(raw)?;

        match self
            .pathfinder
            .query_registered("FetchJam", json!({ "jamId": session.id }))
            .await
        {
            Ok(meta) => {
                debug!("pathfinder enriched jam {}", session.id);
                if let Some(members) = meta
                    .get("jam")
                    .and_then(|j| j.get("members"))
                    .and_then(|m| m.as_array())
                {
                    for member in members {
                        let Some(mid) = member
                            .get("user")
                            .and_then(|u| u.get("uri"))
                            .and_then(|u| u.as_str())
                        else {
                            continue;
                        };
                        if let Some(slot) = session.members.iter_mut().find(|m| m.id == mid) {
                            slot.name = member
                                .get("user")
                                .and_then(|u| u.get("name"))
                                .and_then(|n| n.as_str())
                                .map(str::to_owned);
                        }
                    }
                }
            }
            Err(e) => warn!("pathfinder enrichment skipped for jam {}: {e}", session.id),
        }
        Ok(session)
    }
}

/// Extracts the join token from whatever the user pasted: an invite link
/// (`https://open.spotify.com/socialsession/<token>`, query string and all),
/// a `spotify:socialsession:<token>` URI, or the bare token.
///
/// The join endpoint takes the *token*, not the session id, and pasting the
/// link is the only way most users ever see one — so accept the link.
///
/// > Until 2026-08 this looked for the literal segment `socialsession` and
/// > fell back to the whole input when it was absent. Spotify hands out
/// > several link shapes — `/jam/<token>`, a `spotify.link` shortlink, a
/// > locale-prefixed path — and on every one of those the fallback then split
/// > on `/` and returned **`https:`**, which the join endpoint received as the
/// > token. Matching on a known segment name is the fragile part, so this now
/// > takes the last non-empty path segment regardless of what precedes it.
pub fn join_token(input: &str) -> &str {
    let input = input.trim();
    // Drop the scheme so `https://` does not read as an empty path segment
    // followed by a host; `spotify:socialsession:<token>` has no `://` and
    // falls through with its colons intact, which the rsplit below handles.
    let path = match input.split_once("://") {
        Some((_, rest)) => rest,
        None => input,
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    path.rsplit(['/', ':'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(path)
}

fn value<'a>(raw: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| raw.get(*name))
}

fn string(raw: &Value, names: &[&str]) -> Option<String> {
    value(raw, names).and_then(Value::as_str).map(str::to_owned)
}

fn boolean(raw: &Value, names: &[&str]) -> bool {
    value(raw, names).and_then(Value::as_bool).unwrap_or(false)
}

/// Decodes a member from either protobuf-JSON snake_case or Web API
/// camelCase. Internal services have used both representations.
pub(crate) fn member_from_value(raw: &Value, owner_id: Option<&str>) -> JamMember {
    let id = string(raw, &["id"]).unwrap_or_default();
    let username = string(raw, &["username"]);
    let display_name = string(raw, &["display_name", "displayName"]);
    JamMember {
        name: display_name.clone().or_else(|| username.clone()),
        is_host: owner_id.is_some_and(|owner| owner == id),
        id,
        username,
        display_name,
        image_url: string(raw, &["image_url", "imageUrl"]),
        large_image_url: string(raw, &["large_image_url", "largeImageUrl"]),
        joined_timestamp: value(raw, &["joined_timestamp", "joinedTimestamp", "timestamp"])
            .and_then(Value::as_i64),
        is_listening: boolean(raw, &["is_listening", "isListening"]),
        is_controlling: boolean(raw, &["is_controlling", "isControlling"]),
        playback_control: string(raw, &["playbackControl", "playback_control"]),
        is_current_user: boolean(raw, &["is_current_user", "isCurrentUser"]),
    }
}

/// True only for links a user can actually open or paste somewhere.
///
/// Deliberately scheme-based rather than a full URL parse: the failure being
/// guarded against is Spotify handing back `hs://`/`hm://` internal endpoints,
/// not a malformed https URL.
fn is_shareable_web_url(url: &str) -> bool {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://")) && url.len() > 8
}

/// Maps a v2/v3 social-connect session response onto the stable application
/// model while retaining the untouched payload for newly introduced fields.
pub(crate) fn session_from_value(raw: Value) -> Result<JamSession, JamError> {
    let raw = raw.get("jam").cloned().unwrap_or(raw);
    let id = string(&raw, &["session_id", "sessionId", "id"])
        .filter(|id| !id.is_empty())
        .ok_or_else(|| JamError::DecodeMessage("session payload has no session_id".into()))?;
    let owner_id = string(&raw, &["session_owner_id", "sessionOwnerId", "ownerId"]);
    let members = value(&raw, &["session_members", "sessionMembers", "members"])
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| member_from_value(item, owner_id.as_deref()))
                .filter(|member| !member.id.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let host = members.iter().find(|member| member.is_host).cloned();
    let queue_only_mode = boolean(&raw, &["queue_only_mode", "queueOnlyMode"]);
    let join_token = string(
        &raw,
        &["join_session_token", "joinSessionToken", "joinToken"],
    );
    // The official app's "Copy Jam link" shares a shortened `spotify.link/…`
    // URL, not the raw `open.spotify.com/socialsession/<token>` form — if
    // Spotify's own response carries that shortened link under some key,
    // prefer it. The extra candidates below are speculative (unconfirmed
    // against a live response); the constructed fallback is the one two
    // independent reverse-engineering projects (this one's own PyPI capture
    // and a separate OSS Spicetify extension) confirmed actually works, so
    // it stays last and is never removed even if a new field name is added
    // above it.
    let join_uri = string(&raw, &["join_session_uri", "joinSessionUri", "joinUri"]).or_else(|| {
        join_token
            .as_ref()
            .map(|token| format!("spotify:socialsession:{token}"))
    });
    let join_url = string(
        &raw,
        &[
            "join_session_url",
            "joinSessionUrl",
            "joinUrl",
            "share_url",
            "shareUrl",
            "invite_link",
            "inviteLink",
            "link",
        ],
    )
    // Several of those keys are speculative, and social-connect responses have
    // been observed carrying Spotify's own internal transport URIs (`hm://`,
    // `hs://`) under generically-named fields like `link`. Those are not
    // shareable: pasted into a browser or a chat client they resolve to
    // nothing. Only an http(s) URL is worth handing to the user, so anything
    // else falls through to the constructed open.spotify.com form below —
    // which is the shape that is actually confirmed to work.
    .filter(|url| is_shareable_web_url(url))
    .or_else(|| {
        join_token
            .as_ref()
            .map(|token| format!("https://open.spotify.com/socialsession/{token}"))
    });
    if join_token.is_some()
        && string(
            &raw,
            &[
                "join_session_url",
                "joinSessionUrl",
                "joinUrl",
                "share_url",
                "shareUrl",
            ],
        )
        .filter(|url| is_shareable_web_url(url))
        .is_none()
    {
        // Diagnostic only, not a functional problem: the constructed link
        // does work (see the comment above). This exists so a live session
        // can confirm, from the log, whether Spotify's response genuinely
        // never carries a shortened link or Rustify is just looking under
        // the wrong key.
        if let Some(object) = raw.as_object() {
            debug!(
                target: "jams::session",
                "no shortened join URL field in session response; constructing one from join_session_token. Top-level keys present: {:?}",
                object.keys().collect::<Vec<_>>()
            );
        }
    }
    let session_type = value(
        &raw,
        &["initialSessionType", "initial_session_type", "sessionType"],
    )
    .and_then(|v| {
        v.as_str()
            .map(str::to_owned)
            .or_else(|| v.as_i64().map(|n| n.to_string()))
    });

    Ok(JamSession {
        id,
        timestamp: value(&raw, &["timestamp"]).and_then(Value::as_i64),
        owner_id,
        host,
        members,
        queue: Vec::new(),
        active_state: raw.clone(),
        join_token,
        join_url,
        join_uri,
        is_session_owner: boolean(&raw, &["is_session_owner", "isSessionOwner"]),
        is_listening: boolean(&raw, &["is_listening", "isListening"]),
        is_controlling: boolean(&raw, &["is_controlling", "isControlling"]),
        is_discoverable: boolean(&raw, &["is_discoverable", "isDiscoverable"]),
        session_type,
        host_active_device_id: string(&raw, &["host_active_device_id", "hostActiveDeviceId"]),
        max_member_count: value(&raw, &["maxMemberCount", "max_member_count"])
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
        active: boolean(&raw, &["active"]),
        queue_only_mode,
        queue_control_allowed: !queue_only_mode,
        wifi_broadcast: boolean(&raw, &["wifi_broadcast", "wifiBroadcast"]),
        host_device_info: value(&raw, &["host_device_info", "hostDeviceInfo"]).cloned(),
    })
}

impl Drop for JamManager {
    fn drop(&mut self) {
        if let Some(task) = self.dealer_task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_social_connect_session() {
        let session = session_from_value(json!({
            "timestamp": 123,
            "session_id": "session-1",
            "join_session_token": "token-1",
            "session_owner_id": "owner",
            "session_members": [{
                "id": "owner",
                "username": "owner-user",
                "display_name": "Owner",
                "image_url": "small",
                "large_image_url": "large",
                "is_listening": true,
                "is_controlling": true,
                "is_current_user": true
            }],
            "is_session_owner": true,
            "is_discoverable": true,
            "initialSessionType": "REMOTE_V2",
            "queue_only_mode": true,
            "active": true
        }))
        .unwrap();

        assert_eq!(session.id, "session-1");
        assert_eq!(
            session.join_uri.as_deref(),
            Some("spotify:socialsession:token-1")
        );
        assert_eq!(
            session.host.as_ref().and_then(|m| m.name.as_deref()),
            Some("Owner")
        );
        assert!(!session.queue_control_allowed);
        assert!(session.is_session_owner);
        assert!(session.members[0].is_current_user);
    }

    #[test]
    fn rejects_payload_without_session_id() {
        assert!(matches!(
            session_from_value(json!({"active": true})),
            Err(JamError::DecodeMessage(_))
        ));
    }

    #[test]
    fn discards_non_web_join_urls_and_constructs_a_shareable_one() {
        // Spotify has been seen returning its own internal transport URIs
        // under the generic `link`/`share_url` keys. Handing one of those to
        // the user produces a "Copy link" that opens nothing anywhere.
        for internal in ["hs://jam/token-1", "hm://social-connect/v2/token-1"] {
            let session = session_from_value(json!({
                "session_id": "session-1",
                "join_session_token": "token-1",
                "link": internal,
            }))
            .unwrap();
            assert_eq!(
                session.join_url.as_deref(),
                Some("https://open.spotify.com/socialsession/token-1")
            );
        }
    }

    #[test]
    fn keeps_a_real_shortened_share_url() {
        let session = session_from_value(json!({
            "session_id": "session-1",
            "join_session_token": "token-1",
            "share_url": "https://spotify.link/abc123",
        }))
        .unwrap();
        assert_eq!(
            session.join_url.as_deref(),
            Some("https://spotify.link/abc123")
        );
    }

    #[test]
    fn extracts_join_tokens_from_every_supported_invite_shape() {
        assert_eq!(join_token("bare-token"), "bare-token");
        assert_eq!(join_token("spotify:socialsession:uri-token"), "uri-token");
        assert_eq!(
            join_token("https://open.spotify.com/socialsession/web-token?si=ignored"),
            "web-token"
        );
        assert_eq!(
            join_token("https://open.spotify.com/intl-it/jam/new-shape#fragment"),
            "new-shape"
        );
    }
}
