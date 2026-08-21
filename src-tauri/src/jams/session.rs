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

/// A member of a jam, decoded best-effort from the API response. Field names
/// follow the common casing; confirm against a live capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JamMember {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub is_host: bool,
}

/// A track in a jam queue, decoded best-effort.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct JamSession {
    pub id: String,
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
        let (dealer, events) = DealerClient::new(
            config.dealer_url.clone(),
            connection_id,
            access_token,
            client_token,
            256,
        );
        let dealer_task = Some(dealer.spawn());
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
    pub async fn pathfinder_query(&self, operation_name: &str, variables: Value) -> Result<Value, JamError> {
        self.pathfinder.query_registered(operation_name, variables).await
    }

    pub async fn create(&self, context_uri: Option<&str>) -> Result<JamSession, JamError> {
        let resp = self.api.create_jam(context_uri).await?;
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

    /// Returns the dealer event stream. Ownership transfers to the caller, so
    /// call this exactly once and pass the receiver onward.
    pub fn events(&self) -> mpsc::Receiver<JamEvent> {
        self.events
            .lock()
            .expect("jam events mutex poisoned")
            .take()
            .expect("events() may only be called once; pass the returned receiver around")
    }

    /// Enriches a raw jam payload with member metadata from Pathfinder when a
    /// `FetchJam` persisted query is registered; otherwise returns the raw
    /// shape unchanged. Pathfinder failure is logged, not fatal — the SpClient
    /// response already carries the jam id.
    async fn hydrate(&self, raw: Value) -> Result<JamSession, JamError> {
        let mut session: JamSession = serde_json::from_value(normalize(raw))?;

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

/// Maps a social-connect session payload onto [`JamSession`]'s shape.
///
/// The service names its fields `session_id` / `session_members` /
/// `session_owner_id`; deserialising it directly yields a session with an
/// empty id and no members, which the UI shows as a jam that exists but has
/// nobody in it. A payload already in our shape (or wrapped in a `jam` key)
/// passes through untouched.
fn normalize(raw: Value) -> Value {
    let raw = raw.get("jam").cloned().unwrap_or(raw);
    let Some(id) = raw.get("session_id").and_then(Value::as_str) else {
        return raw;
    };
    let owner = raw
        .get("session_owner_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let members: Vec<Value> = raw
        .get("session_members")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|m| {
                    let mid = m.get("id").and_then(Value::as_str).unwrap_or_default();
                    json!({
                        "id": mid,
                        "name": m
                            .get("display_name")
                            .or_else(|| m.get("username"))
                            .and_then(Value::as_str),
                        "isHost": mid == owner,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "id": id,
        "host": members.iter().find(|m| m["isHost"] == json!(true)).cloned(),
        "members": members,
        // The queue is the Connect queue, which this payload does not carry.
        "queue": [],
        "joinToken": raw.get("join_session_token"),
        "joinUrl": raw.get("join_session_url"),
        // Keep the untouched payload: it holds fields we do not model yet
        // (`active`, `host_active_device_id`, per-member flags).
        "activeState": raw,
    })
}

impl Drop for JamManager {
    fn drop(&mut self) {
        if let Some(task) = self.dealer_task.take() {
            task.abort();
        }
    }
}