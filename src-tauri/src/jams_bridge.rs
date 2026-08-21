//! Glue between the experimental `jams` module and the app's session.
//!
//! Owns one [`JamManager`] (and therefore one dealer listener) for the life of
//! the controller, feeds it the Web API bearer token that lives in
//! [`crate::state::TokenStore`], and forwards dealer events to the webview as
//! `jams:changed` (see [`crate::state::events::JAMS`]).
//!
//! The controller is built lazily on the first jam command and dropped on
//! logout, which aborts the dealer task and ends the forward loop.

use std::sync::Arc;

use futures_util::StreamExt;
use librespot::core::dealer::protocol::{Message as DealerMessage, PayloadValue};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use librespot::core::session::Session;

use crate::jams::{
    session::{member_from_value, session_from_value},
    AccessToken, ClientIdentity, ConnectionId, JamConfig, JamCredentials, JamError, JamEvent,
    JamManager, JamSession,
};
use crate::state::{events, TokenStore};

/// Pulls every credential a jam call needs out of the live librespot session.
///
/// A free function rather than a method so [`JamController::build`] can run it
/// before the controller (and its dealer task) exists.
async fn refresh(
    spotify: &Session,
    tokens: &TokenStore,
    token: &AccessToken,
    web_token: &AccessToken,
    client_token: &AccessToken,
    connection_id: &ConnectionId,
) {
    web_token.set(tokens.get().await);

    // NOT the Web API bearer: spclient's RBAC filter refuses a token minted
    // for a self-registered Client ID with `403 RBAC: access denied` whatever
    // its scopes, because social-connect is first-party only.
    //
    // login5, not keymaster. `hm://keymaster/token/authenticated` answers 403
    // "Invalid request" for an OAuth-authenticated session — Spotify retired
    // that path — and librespot's own `SpClient` moved to
    // `login5().auth_token()`, which mints under the same desktop client id
    // that produces the `client-token` below. One client, three credentials
    // that agree.
    match spotify.login5().auth_token().await {
        Ok(token_data) => token.set(token_data.access_token),
        Err(e) => log::warn!("jams: no first-party access token from librespot login5: {e}"),
    }

    match spotify.spclient().client_token().await {
        Ok(value) => client_token.set(value),
        Err(e) => log::warn!("jams: could not obtain client-token from librespot: {e}"),
    }

    // Assigned on librespot's dealer connection, which is also where the
    // Connect device lives — so the jam, the device and the push channel all
    // agree. Empty until Spirc has seen it; the header is then omitted rather
    // than faked.
    let conn = spotify.connection_id();
    if conn.is_empty() {
        log::warn!("jams: no Spotify-Connection-Id yet; jam calls may be refused");
    }
    connection_id.set(conn);
}

fn dealer_json(message: DealerMessage) -> Result<Value, JamError> {
    match message.payload {
        PayloadValue::Json(json) => serde_json::from_str(&json).map_err(JamError::from),
        PayloadValue::Raw(bytes) => serde_json::from_slice(&bytes).map_err(JamError::from),
        PayloadValue::Empty => Err(JamError::DecodeMessage(
            "social-connect dealer update had an empty payload".into(),
        )),
    }
}

fn session_update_event(raw: Value) -> Result<JamEvent, JamError> {
    let reason = raw
        .get("reason")
        .and_then(|value| {
            value.as_str().map(str::to_owned).or_else(|| {
                value.as_i64().map(|number| {
                    match number {
                        1 => "NEW_SESSION",
                        2 => "USER_JOINED",
                        3 => "USER_LEFT",
                        4 => "SESSION_DELETED",
                        5 => "YOU_LEFT",
                        6 => "YOU_WERE_KICKED",
                        7 => "YOU_JOINED",
                        8 => "PARTICIPANT_PROMOTED_TO_HOST",
                        9 => "DISCOVERABILITY_CHANGED",
                        10 => "USER_KICKED",
                        _ => "UNKNOWN_UPDATE_TYPE",
                    }
                    .to_owned()
                })
            })
        })
        .unwrap_or_else(|| "UNKNOWN_UPDATE_TYPE".to_owned());
    let session = raw
        .get("session")
        .cloned()
        .map(session_from_value)
        .transpose()?;
    let owner_id = session
        .as_ref()
        .and_then(|session| session.owner_id.as_deref());
    let updated_members = raw
        .get("updated_session_members")
        .or_else(|| raw.get("updatedSessionMembers"))
        .and_then(Value::as_array)
        .map(|members| {
            members
                .iter()
                .map(|member| member_from_value(member, owner_id))
                .collect()
        })
        .unwrap_or_default();
    Ok(JamEvent::SessionUpdate {
        reason,
        session: session.map(Box::new),
        updated_members,
    })
}

fn terminal_update(reason: &str) -> bool {
    matches!(reason, "SESSION_DELETED" | "YOU_LEFT" | "YOU_WERE_KICKED")
}

pub struct JamController {
    manager: JamManager,
    /// The live librespot session. Jams need three things only it has: the
    /// Connect device id, a real `client-token`, and the dealer connection id
    /// the server assigned. See [`Self::sync_token`].
    spotify: Session,
    /// Bearer for spclient/Pathfinder: librespot's keymaster token, issued
    /// under Spotify's own desktop client id. Refreshed before every command
    /// via [`Self::sync_token`].
    token: AccessToken,
    /// Bearer for public Web API calls — the app's own, from `TokenStore`, so
    /// that traffic stays on the user's quota.
    web_token: AccessToken,
    /// Mirrors librespot's `client-token` in the same way.
    client_token: AccessToken,
    /// Mirrors librespot's `Spotify-Connection-Id`.
    connection_id: ConnectionId,
    config: JamConfig,
    session: Arc<tokio::sync::RwLock<Option<JamSession>>>,
    /// Kept only so the task is dropped with the controller; the loop ends on
    /// its own once the dealer's sender is gone.
    _forward: tauri::async_runtime::JoinHandle<()>,
}

impl JamController {
    /// Builds the controller from `jams.toml` in the app data dir (defaults if
    /// absent) and spawns the dealer listener plus the event forwarder.
    ///
    /// Takes the live librespot [`Session`] because a jam is not something a
    /// standalone HTTP client can open: Spotify binds it to a Connect device
    /// and to the dealer connection that device is registered on, and demands
    /// a `client-token` minted through a protobuf handshake. librespot already
    /// has all three; inventing our own produced a bare 400 on every call.
    /// `async` so the credentials are in place *before* the dealer task is
    /// spawned: it reaches for them on its first connect, and an empty cell
    /// costs a failed connect plus a backoff cycle of log noise.
    pub async fn build(app: &AppHandle, session: Session, tokens: &TokenStore) -> AppResult<Self> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

        let config = JamConfig::from_file(data_dir.join("jams.toml")).unwrap_or_default();

        let identity = ClientIdentity {
            device_id: session.device_id().to_owned(),
            // The same client id the streaming session uses: the client-token
            // mint only accepts ids it knows, and self-registered ones are not
            // among them.
            client_id: crate::auth::streaming_client_id(),
            ..ClientIdentity::default()
        };

        let token = AccessToken::default();
        let web_token = AccessToken::default();
        let client_token = AccessToken::default();
        let connection_id = ConnectionId::new();
        refresh(
            &session,
            tokens,
            &token,
            &web_token,
            &client_token,
            &connection_id,
        )
        .await;

        let manager = JamManager::new_http_only(
            reqwest::Client::new(),
            config.clone(),
            JamCredentials {
                identity,
                access_token: Arc::new(token.clone()),
                web_api_token: Some(Arc::new(web_token.clone())),
                client_token: Some(client_token.clone()),
                connection_id: connection_id.clone(),
            },
        )
        .map_err(AppError::from)?;

        // Reuse librespot's authenticated Dealer connection. Opening a second
        // websocket here would create a different server-assigned connection
        // id than the Connect device and social-connect HTTP requests use.
        let mut session_updates = session
            .dealer()
            .add_listen_for("social-connect/v2/session_update")
            .map_err(|error| {
                AppError::Other(format!("subscribe to Jam session updates: {error}"))
            })?;
        let mut broadcast_updates = session
            .dealer()
            .add_listen_for("social-connect/v2/broadcast_status_update")
            .map_err(|error| {
                AppError::Other(format!("subscribe to Jam broadcast updates: {error}"))
            })?;
        let session_state = Arc::new(tokio::sync::RwLock::new(None));
        let state_for_updates = session_state.clone();
        let app = app.clone();
        let forward = tauri::async_runtime::spawn(async move {
            let mut sessions_open = true;
            let mut broadcasts_open = true;
            while sessions_open || broadcasts_open {
                tokio::select! {
                    message = session_updates.next(), if sessions_open => match message {
                        Some(message) => match dealer_json(message).and_then(session_update_event) {
                            Ok(event) => {
                                if let JamEvent::SessionUpdate { reason, session, .. } = &event {
                                    let next = if terminal_update(reason) {
                                        None
                                    } else {
                                        session.as_deref().cloned()
                                    };
                                    if terminal_update(reason) || next.is_some() {
                                        *state_for_updates.write().await = next;
                                    }
                                }
                                let _ = app.emit(events::JAMS, &event);
                            }
                            Err(error) => log::warn!("jams: invalid session_update: {error}"),
                        },
                        None => sessions_open = false,
                    },
                    message = broadcast_updates.next(), if broadcasts_open => match message {
                        Some(message) => match dealer_json(message) {
                            Ok(payload) => {
                                let _ = app.emit(events::JAMS, JamEvent::BroadcastStatus(payload));
                            }
                            Err(error) => log::warn!("jams: invalid broadcast_status_update: {error}"),
                        },
                        None => broadcasts_open = false,
                    },
                }
            }
        });

        Ok(Self {
            manager,
            spotify: session,
            token,
            web_token,
            client_token,
            connection_id,
            config,
            session: session_state,
            _forward: forward,
        })
    }

    pub fn config(&self) -> &JamConfig {
        &self.config
    }

    /// Copies the current credentials out of the app session into the jams
    /// module. Call before any command: they rotate independently, and a stale
    /// one fails the request with no useful message.
    pub async fn sync_token(&self, tokens: &TokenStore) {
        refresh(
            &self.spotify,
            tokens,
            &self.token,
            &self.web_token,
            &self.client_token,
            &self.connection_id,
        )
        .await;
    }

    pub async fn create(&self, context_uri: Option<&str>) -> Result<JamSession, JamError> {
        let session = self.manager.create(context_uri).await?;
        *self.session.write().await = Some(session.clone());
        Ok(session)
    }

    pub async fn join(&self, jam_id: &str) -> Result<JamSession, JamError> {
        let session = self.manager.join(jam_id).await?;
        *self.session.write().await = Some(session.clone());
        Ok(session)
    }

    pub async fn refresh_session(&self) -> Result<JamSession, JamError> {
        let session = self.manager.current().await?;
        *self.session.write().await = Some(session.clone());
        Ok(session)
    }

    pub async fn leave(&self) -> Result<(), JamError> {
        let Some(session) = self.session().await else {
            return Err(JamError::JamNotFound(
                "no active jam; create or join one first".into(),
            ));
        };
        self.manager.leave(&session.id).await?;
        *self.session.write().await = None;
        Ok(())
    }

    pub async fn add_track(&self, track_uri: &str) -> Result<(), JamError> {
        let Some(session) = self.session().await else {
            return Err(JamError::JamNotFound(
                "no active jam; create or join one first".into(),
            ));
        };
        self.manager.add_track(&session.id, track_uri).await
    }

    pub async fn set_queue_control(&self, allowed: bool) -> Result<JamSession, JamError> {
        let session = self.manager.set_queue_control(allowed).await?;
        *self.session.write().await = Some(session.clone());
        Ok(session)
    }

    pub async fn kick(&self, member_id: &str) -> Result<JamSession, JamError> {
        let Some(current) = self.session().await else {
            return Err(JamError::JamNotFound(
                "no active jam; create or join one first".into(),
            ));
        };
        if !current.is_session_owner {
            return Err(JamError::PermissionDenied(
                "only the Jam host can remove participants".into(),
            ));
        }
        let session = self.manager.kick(&current.id, member_id).await?;
        *self.session.write().await = Some(session.clone());
        Ok(session)
    }

    pub async fn end(&self) -> Result<(), JamError> {
        let Some(current) = self.session().await else {
            return Err(JamError::JamNotFound(
                "no active jam; create one first".into(),
            ));
        };
        if !current.is_session_owner {
            return Err(JamError::PermissionDenied(
                "only the Jam host can end the session".into(),
            ));
        }
        self.manager.end(&current.id).await?;
        *self.session.write().await = None;
        Ok(())
    }

    pub async fn session(&self) -> Option<JamSession> {
        self.session.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_authoritative_session_update() {
        let event = session_update_event(json!({
            "reason": "USER_JOINED",
            "session": {
                "session_id": "jam-1",
                "session_owner_id": "host",
                "session_members": [{"id": "host", "display_name": "Host"}]
            },
            "updated_session_members": [{"id": "guest", "display_name": "Guest"}]
        }))
        .unwrap();

        match event {
            JamEvent::SessionUpdate {
                reason,
                session: Some(session),
                updated_members,
            } => {
                assert_eq!(reason, "USER_JOINED");
                assert_eq!(session.id, "jam-1");
                assert_eq!(updated_members[0].display_name.as_deref(), Some("Guest"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn maps_numeric_terminal_reason() {
        let event = session_update_event(json!({"reason": 6})).unwrap();
        match event {
            JamEvent::SessionUpdate { reason, .. } => {
                assert_eq!(reason, "YOU_WERE_KICKED");
                assert!(terminal_update(&reason));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
