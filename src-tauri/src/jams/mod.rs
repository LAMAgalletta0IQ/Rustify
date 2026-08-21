//! Experimental Spotify Jams (social listening sessions) module.
//!
//! Self-contained by design: it depends only on standard crates and exposes
//! [`token::TokenProvider`] as the single seam where the host app's OAuth
//! session plugs in. Nothing here imports from the rest of the crate, so it
//! can be developed and exercised on its own (`examples/jam_demo.rs`) before
//! being wired into the app.
//!
//! The three internal services it talks to:
//!
//! - **SpClient** (HTTPS) — request/response actions: create/join/update/leave
//!   a jam. Endpoint paths are *not* hardcoded; they are captured from the
//!   official client and supplied through [`config::JamConfig`].
//! - **Dealer** (WebSocket) — pushes real-time events. Automatic ping/pong and
//!   exponential-backoff reconnection live in [`dealer::DealerClient`].
//! - **Pathfinder** (GraphQL) — metadata via persisted queries (operation name
//!   + SHA-256 hash). Hashes are config-driven and expire.
//!
//! See `README_jams.md` for capture and maintenance notes.

pub mod client_token;
pub mod config;
pub mod dealer;
pub mod error;
pub mod pathfinder;
pub mod session;
pub mod spclient;
pub mod token;

pub use client_token::{ClientToken, ClientTokenManager};
pub use config::JamConfig;
pub use dealer::{DealerClient, JamEvent};
pub use error::JamError;
pub use pathfinder::PathfinderClient;
pub use session::{join_token, JamCredentials, JamManager, JamMember, JamSession, JamTrack};
pub use spclient::JamApiClient;
pub use token::{AccessToken, TokenProvider};

/// Who this client claims to be when talking to Spotify's internal services.
///
/// Two of those services care:
///
/// - the **client-token** mint rejects a `client_id` it does not recognise, so
///   this must be a real Spotify client id (the app passes librespot's desktop
///   one, which is also what the streaming session uses);
/// - **social-connect** — the service behind Jams — keys a session to a
///   `local_device_id`, which has to be *this app's Connect device id* for the
///   jam to attach to the player the user is actually listening on. A random
///   id creates a jam bound to a device that does not exist.
///
/// [`Default`] is for standalone use (`examples/jam_demo.rs`): a random device
/// id, which is fine for minting a token and for the dealer, but not for
/// creating a jam that should control real playback.
#[derive(Clone, Debug)]
pub struct ClientIdentity {
    /// Connect device id. For the app, `librespot`'s `Session::device_id()`.
    pub device_id: String,
    /// Spotify client id recognised by the client-token mint.
    pub client_id: String,
    /// Version string reported to the mint; only loosely validated.
    pub client_version: String,
}

impl Default for ClientIdentity {
    fn default() -> Self {
        Self {
            device_id: uuid::Uuid::new_v4().as_simple().to_string(),
            // Spotify's desktop client id — the same value librespot's
            // `SessionConfig::default()` carries. Not a secret; it travels in
            // the clear on every request the official client makes.
            client_id: "65b708073fc0480ea92a077233ca87bd".to_owned(),
            client_version: "1.2.52.442".to_owned(),
        }
    }
}

/// Shared `Spotify-Connection-Id` tying SpClient/Pathfinder HTTPS requests to
/// the dealer websocket the server should push updates over.
///
/// **The server assigns this id; a client cannot choose it.** It arrives on
/// the dealer connection (`hm://pusher/v1/connections/`, in the message's
/// `Spotify-Connection-Id` header) and the HTTPS clients echo it back. The
/// host app writes the current value with [`ConnectionId::set`].
///
/// > Until 2026-08 this generated a random UUID and the dealer "rotated" it on
/// > every connect, on the assumption that the client picks the id and
/// > announces it in the handshake. It does not, so every jam request carried
/// > an id belonging to no connection — which social-connect answers with a
/// > bare **400**. Rustify now supplies librespot's own connection id
/// > (`Session::connection_id()`), which has the further advantage of matching
/// > the device in `local_device_id`: same device, same connection.
///
/// Empty until the host supplies one, in which case the HTTPS clients omit the
/// header rather than sending a made-up value.
#[derive(Clone, Debug, Default)]
pub struct ConnectionId {
    inner: std::sync::Arc<std::sync::RwLock<String>>,
}

impl ConnectionId {
    /// Starts empty: nothing may invent an id.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current id, or an empty string when none has been supplied (or the
    /// lock is poisoned); the HTTPS clients then omit the header.
    pub fn current(&self) -> String {
        self.inner.read().map(|guard| guard.clone()).unwrap_or_default()
    }

    /// Records the id the server assigned. Called by the host app whenever it
    /// learns one, which for Rustify is before every jam command.
    pub fn set(&self, id: impl Into<String>) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = id.into();
        }
    }
}
