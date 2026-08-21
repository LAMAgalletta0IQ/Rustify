use std::sync::Arc;

use librespot::connect::Spirc;
use librespot::core::session::Session;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, RwLock};

use crate::audio::{AudioRuntime, StreamQuality};

/// Event names emitted to the webview. Keep in sync with `src/lib/events.ts`.
pub mod events {
    pub const PLAYBACK: &str = "playback:changed";
    pub const AUTH: &str = "auth:changed";
    pub const JAMS: &str = "jams:changed";
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
    /// Requested local-session quality. `audio_quality_label` is the concrete
    /// bitrate librespot 0.8 maps it to; remote-device quality is unknowable.
    pub audio_quality: StreamQuality,
    pub audio_quality_label: String,

    /// Position as of [`Self::position_at`], and when that reading was taken.
    ///
    /// Not serialised — they exist so `position_ms` can be recomputed on
    /// demand. librespot reports a position only on some events (`Playing`,
    /// `Seeked`, periodic corrections); volume and shuffle changes carry none.
    /// Without this, emitting a snapshot after one of those would ship the last
    /// reported position — normally 0, from the start of the track — and reset
    /// the UI's clock while audio kept playing.
    #[serde(skip)]
    pub position_base_ms: u32,
    #[serde(skip)]
    pub position_at: Option<std::time::Instant>,
}

impl PlaybackState {
    /// Anchors the position so elapsed time can be added to it later.
    pub fn set_position(&mut self, position_ms: u32) {
        self.position_ms = position_ms;
        self.position_base_ms = position_ms;
        self.position_at = Some(std::time::Instant::now());
    }

    /// Brings `position_ms` up to date from the anchor. Call before handing a
    /// snapshot to the UI.
    pub fn refresh_position(&mut self) {
        if !self.is_playing {
            return;
        }
        let Some(at) = self.position_at else { return };
        let elapsed = at.elapsed().as_millis().min(u32::MAX as u128) as u32;
        let pos = self.position_base_ms.saturating_add(elapsed);
        self.position_ms = if self.duration_ms > 0 {
            pos.min(self.duration_ms)
        } else {
            pos
        };
    }
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
    /// Mirrors playback from other Connect devices; aborted alongside the
    /// refresher, or it would keep polling after logout with a dead token.
    pub remote_task: tauri::async_runtime::JoinHandle<()>,
}

/// Canonical, authoritative signal for whether this app is the active
/// Spotify Connect device.
///
/// `PlaybackState::is_active_device` mirrors this for the webview snapshot,
/// but this channel is the source of truth: it is written from exactly two
/// places in `player::spawn_event_pump` — `SessionConnected`/`Playing`/
/// `Loading`/`Paused` (true) and `SessionDisconnected` (false), the latter
/// fired by librespot's own Spirc when a Connect cluster update reports
/// another device took over (see `librespot_connect::spirc::handle_cluster_update`,
/// a fully public/supported code path — no private endpoint is called to
/// learn this). Anything that needs to react to activation/deactivation
/// (rather than poll `AppState::playback`) should `subscribe()` here.
pub struct ActiveDeviceSignal {
    tx: watch::Sender<bool>,
}

impl ActiveDeviceSignal {
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.tx.subscribe()
    }

    pub fn set(&self, active: bool) {
        self.tx.send_if_modified(|v| {
            let changed = *v != active;
            *v = active;
            changed
        });
    }
}

impl Default for ActiveDeviceSignal {
    fn default() -> Self {
        Self {
            tx: watch::channel(false).0,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub spotify: RwLock<Option<SpotifySession>>,
    pub playback: RwLock<PlaybackState>,
    pub auth: RwLock<AuthState>,
    /// Held outside `spotify` so the event pump can read it without taking a
    /// lock on the whole session.
    pub tokens: TokenStore,
    /// Synchronous audio-thread controls. This is separate from the Tokio
    /// state so the sink never blocks on an async runtime lock.
    pub audio: AudioRuntime,
    pub active_device: ActiveDeviceSignal,
    /// Experimental jams session, built lazily on the first jam command and
    /// dropped on logout. `Arc` so commands clone the handle instead of holding
    /// the read guard across awaits.
    pub jams: RwLock<Option<Arc<crate::jams_bridge::JamController>>>,
    /// Cached first-party clients (Pathfinder now, other internal services as
    /// they are enabled). Credentials are supplied per request and never kept.
    pub internal_spotify: crate::spotify::InternalSpotify,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
}
