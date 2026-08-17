use std::sync::Arc;

use librespot::connect::Spirc;
use librespot::core::session::Session;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Event names emitted to the webview. Keep in sync with `src/lib/events.ts`.
pub mod events {
    pub const PLAYBACK: &str = "playback:changed";
    pub const AUTH: &str = "auth:changed";
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub uri: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub cover_url: Option<String>,
    pub duration_ms: u32,
}

/// The single snapshot of playback pushed to the frontend on every change.
///
/// Deliberately flat and cheap to clone: it is serialised on every position
/// correction, and the target hardware is a Pentium.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackState {
    pub is_playing: bool,
    pub is_loading: bool,
    /// True while this app is the active Connect device. When false, playback
    /// is happening on some other device and we are acting as a controller.
    pub is_active_device: bool,
    pub track: Option<TrackInfo>,
    pub position_ms: u32,
    pub duration_ms: u32,
    /// librespot volume is u16 (0..=65535). The UI works in 0..=100.
    pub volume: u16,
    pub shuffle: bool,
    pub repeat_context: bool,
    pub repeat_track: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub logged_in: bool,
    pub display_name: Option<String>,
    pub user_id: Option<String>,
    pub product: Option<String>,
    pub avatar_url: Option<String>,
}

/// Shared, mutable Web API bearer token.
///
/// Access tokens expire in ~1 hour, so the token cannot be copied into the
/// tasks that use it — a background refresher swaps the value in place and
/// every holder sees the new one.
#[derive(Clone, Default)]
pub struct TokenStore(Arc<RwLock<String>>);

impl TokenStore {
    pub async fn get(&self) -> String {
        self.0.read().await.clone()
    }

    pub async fn set(&self, token: String) {
        *self.0.write().await = token;
    }
}

/// Live librespot handles. Present only while logged in.
pub struct SpotifySession {
    /// Held to keep the librespot session alive for the login's lifetime.
    #[allow(dead_code)]
    pub session: Session,
    /// Owns the mixer internally, so volume goes through here too.
    pub spirc: Spirc,
    /// Aborted on logout so the refresher does not outlive the session.
    pub refresh_task: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Default)]
pub struct AppState {
    pub spotify: RwLock<Option<SpotifySession>>,
    pub playback: RwLock<PlaybackState>,
    pub auth: RwLock<AuthState>,
    /// Held outside `spotify` so the event pump can read it without taking a
    /// lock on the whole session.
    pub tokens: TokenStore,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}
