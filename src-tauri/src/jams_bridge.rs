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

use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use librespot::core::session::Session;

use crate::jams::{
    AccessToken, ClientIdentity, ConnectionId, JamConfig, JamCredentials, JamError, JamManager,
    JamSession,
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
    session: tokio::sync::RwLock<Option<JamSession>>,
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
        refresh(&session, tokens, &token, &web_token, &client_token, &connection_id).await;

        let manager = JamManager::new(
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

        let mut events = manager.events();
        let app = app.clone();
        let forward = tauri::async_runtime::spawn(async move {
            while let Some(event) = events.recv().await {
                let _ = app.emit(events::JAMS, &event);
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
            session: Default::default(),
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

    pub async fn session(&self) -> Option<JamSession> {
        self.session.read().await.clone()
    }
}