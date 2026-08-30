use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::{collections::HashMap, sync::Arc, time::Instant};

use librespot::connect::Spirc;
use librespot::core::session::Session;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex, RwLock};

use crate::audio::{AudioRuntime, StreamQuality};
use crate::webapi::WebApi;

/// Event names emitted to the webview. Keep in sync with `src/lib/events.ts`.
pub mod events {
    pub const PLAYBACK: &str = "playback:changed";
    pub const AUTH: &str = "auth:changed";
    pub const JAMS: &str = "jams:changed";
    pub const QUEUE: &str = "queue:changed";
    pub const SLEEP_TIMER: &str = "sleep-timer:changed";
    pub const FRIENDS: &str = "friends:changed";
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Recovering,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub uri: String,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_id: Option<String>,
    pub album_uri: Option<String>,
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
    /// Connect context currently supplying the queue. Unlike a playlist name,
    /// this is stable for generated, DJ, radio, collection, and Jam contexts.
    pub context_uri: Option<String>,
    /// Device topology comes directly from the Connect cluster Dealer stream.
    pub active_device: Option<crate::connect::Device>,
    pub available_devices: Vec<crate::connect::Device>,
    pub connection_status: ConnectionStatus,
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
    /// Event-first mirror of the Connect cluster/player/queue protobuf. The
    /// slower Web API task above is only a recovery and metadata fallback.
    pub connect_state_task: tauri::async_runtime::JoinHandle<()>,
    /// Dealer-driven friend-presence feed; optional because this feature must
    /// never make an otherwise healthy login fail.
    pub friends_task: Option<tauri::async_runtime::JoinHandle<()>>,
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
    /// One `reqwest::Client` (internally `Arc`-pooled) for every Web API call
    /// the app makes. Previously every command built its own `WebApi::new()`
    /// — a fresh, empty connection pool per invocation — so back-to-back
    /// requests (Home then Search, a session of artist pages) never reused a
    /// warm TLS connection and paid a full handshake each time.
    pub web_api: WebApi,
    pub spotify: RwLock<Option<SpotifySession>>,
    pub playback: RwLock<PlaybackState>,
    /// Last queue snapshot, hydrated first from Dealer and then (when needed)
    /// by a single Web API request for display metadata.
    pub queue: RwLock<crate::queue::QueueView>,
    /// Small session-local cache: playback events replace frontend snapshots,
    /// but must not trigger another internal lyrics request for the same URI.
    /// The command enforces a fixed upper bound before inserting.
    pub lyrics_cache: RwLock<HashMap<String, crate::lyrics::LyricsResult>>,
    /// Per-track gates prevent the Now Playing view and a concurrent refresh
    /// from issuing duplicate first-party/fallback requests for the same URI.
    pub lyrics_requests: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Per-session format/storage capability snapshots. Storage URLs are never
    /// retained, only safe booleans/counts, so this can have a long TTL.
    pub audio_capability_cache: RwLock<HashMap<String, crate::audio_capabilities::AudioCapability>>,
    pub friend_activity: RwLock<crate::friends::FriendFeed>,
    /// `queryArtistOverview` is the single most expensive, most
    /// rate-limit-exposed call an artist page makes (Pathfinder's quota is
    /// shared with the whole librespot-based client ecosystem, not just this
    /// app — see CLAUDE.md). Cache successful results for
    /// `ARTIST_OVERVIEW_CACHE_TTL` so reopening the same artist doesn't repeat
    /// the round trip, and only successes are cached: a failed lookup is
    /// never remembered as if it were real data. Requests gate the same way
    /// `lyrics_requests` does, so navigating to the same artist twice in
    /// quick succession fires one Pathfinder call, not two.
    pub artist_overview_cache: RwLock<HashMap<String, (Instant, crate::spotify::ArtistOverview)>>,
    pub artist_overview_requests: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Session-local snapshot of the user's saved tracks, so opening several
    /// artist pages in a row does not re-walk the whole library each time to
    /// check which of its tracks the artist owns. Invalidated on any library
    /// save/unsave; otherwise good for `SAVED_TRACKS_CACHE_TTL`.
    pub saved_tracks_cache: RwLock<Option<crate::library::SavedTracksSnapshot>>,
    /// Bounded in-memory ledger of genuine local playback. It never invents
    /// plays and deliberately has no first-party-impersonating Gabo sender.
    pub telemetry: crate::telemetry::TelemetryTracker,
    pub auth: RwLock<AuthState>,
    pub device_auth: crate::auth::DeviceAuthStore,
    pub sleep_timer: crate::sleep_timer::SleepTimerController,
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
    /// Per-artist Last.fm tag cache for the Listening DNA profile. Empty and
    /// untouched unless the user configured a key; cleared when that key
    /// changes, so a rejected key's empty answers are not remembered.
    pub lastfm_cache: crate::lastfm::LastfmCache,
    /// Separate from `web_api`'s client: a different host, and it must carry
    /// no Spotify credential. Kept on state so its connection pool survives
    /// between Profile visits.
    pub lastfm_http: reqwest::Client,
    /// Monotonic id for the live librespot session, bumped by every
    /// `establish` and by `logout`.
    ///
    /// The only consumer is the playback watchdog. Each `PlayerEvent` pump is
    /// tagged with the generation it was spawned for; when its channel closes
    /// it compares that tag against this counter to tell "librespot died under
    /// me, rebuild" apart from "my session was deliberately replaced or signed
    /// out, go away quietly". Without it, every ordinary logout and every
    /// session replacement would close a channel and trip the watchdog into
    /// logging the user straight back in.
    pub session_generation: AtomicU64,
    /// Set while a watchdog rebuild is in flight, so concurrent closures
    /// (the pump and, later, anything else that notices) queue behind one
    /// recovery rather than racing several `establish` calls at once.
    pub session_recovering: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claims the next session generation. Called by `establish` before it
    /// builds anything, and by `logout` so a shutdown-triggered channel
    /// closure cannot look like a crash.
    pub fn next_session_generation(&self) -> u64 {
        self.session_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn session_generation(&self) -> u64 {
        self.session_generation.load(Ordering::SeqCst)
    }
}
