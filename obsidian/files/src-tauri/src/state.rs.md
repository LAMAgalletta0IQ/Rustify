---
tags: [file, backend, state, rust]
---
# `src-tauri/src/state.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~260 lines**

> Until 2026-08 this note described a 127-line file: `PlaybackState` with no
> Connect-cluster fields, `SpotifySession` with two background tasks, and an
> `AppState` with four fields. All three grew substantially as Connect
> mirroring, friend presence, lyrics/audio caching, telemetry, sleep timer,
> Jam, and the first-party services client were added. This note now
> describes the current shape.

## Purpose

Defines all shared application state and the data shapes sent to the webview.
A leaf module: everything depends on it, it depends on nothing else in the
crate. The conceptual overview is in [[state-and-events]].

## Key items

### `mod events`
```rust
pub const PLAYBACK: &str = "playback:changed";
pub const AUTH: &str = "auth:changed";
pub const JAMS: &str = "jams:changed";
pub const QUEUE: &str = "queue:changed";
pub const SLEEP_TIMER: &str = "sleep-timer:changed";
pub const FRIENDS: &str = "friends:changed";
```
Tauri event names. **Must match the constants in [[api.ts]]** — nothing checks
this, and a mismatch silently stops UI updates. Grew from 2 to 6 constants as
Jam, queue, sleep timer, and friend activity each gained their own push
channel instead of piggybacking on `PLAYBACK`.

### `ConnectionStatus`
`Disconnected | Connecting | Connected | Recovering` — surfaced in
`PlaybackState.connection_status` and shown next to the active device name in
[[NowPlaying.svelte]]'s topbar.

### `TrackInfo`
Flattened now-playing metadata: `uri`, `name`, `artists: Vec<String>`,
`album`, `album_id`, `album_uri`, `cover_url: Option<String>`, `duration_ms`.
`album_id`/`album_uri` are what let the player-bar art button and
[[NowPlaying.svelte]]'s artwork open the right [[AlbumView.svelte]]. Populated
by the Web API lookup in [[player.rs]], not by librespot.

### `PlaybackState`
The single snapshot pushed to the UI:

| Field | Notes |
| --- | --- |
| `is_playing`, `is_loading` | Transport |
| `is_active_device` | Mirrors `ActiveDeviceSignal` for the webview — see below for the actual source of truth |
| `track` | `Option<TrackInfo>` |
| `position_ms`, `duration_ms` | |
| `volume` | **Raw `0..=65535`**, librespot's scale — the UI works in 0..=100 |
| `shuffle`, `repeat_context`, `repeat_track` | |
| `context_uri` | Stable across generated/DJ/radio/collection/Jam contexts, unlike a playlist name |
| `active_device`, `available_devices` | Device topology, sourced directly from the Connect cluster Dealer stream (added with [[remote_state.rs]]) |
| `connection_status` | `ConnectionStatus` |
| `audio_quality`, `audio_quality_label` | Requested local-session quality and the concrete bitrate librespot 0.8 maps it to; remote-device quality is unknowable |
| `position_base_ms`, `position_at` | `#[serde(skip)]` — the position anchor |

Deliberately flat and cheap to clone — serialised on every position correction,
and the target hardware is a low-end CPU.

#### `set_position()` / `refresh_position()`

librespot reports a position on only *some* events; `VolumeChanged`,
`ShuffleChanged` and `RepeatChanged` carry none. Because the whole snapshot goes
out on every event, those shipped the **last reported** position — normally 0,
from the start of the track — and the UI clock jumped back to 0:00 while audio
kept playing.

- `set_position(ms)` records the value *and* `Instant::now()`.
- `refresh_position()` adds elapsed wall time, clamped to `duration_ms`, and is
  a no-op while paused.

Every write to `position_ms` goes through `set_position`; every read that leaves
the backend is preceded by `refresh_position`. The two skipped fields never
reach the webview, so [[types.ts]] is unaffected.

### `AuthState`
`logged_in`, `display_name`, `user_id`, `product`, `avatar_url`. `user_id`
matters beyond display: [[Home.svelte]] builds the Liked Songs context URI
`spotify:user:<id>:collection` from it.

### `TokenStore`
```rust
pub struct TokenStore(Arc<RwLock<String>>);
pub async fn get(&self) -> String
pub async fn set(&self, token: String)
```
Shared, mutable Web API bearer token. `Clone` shares the same `Arc`, so a
refresh is visible to every holder at once. This is the **Web API** token
only — the streaming token lives inside librespot's own `Session`. See
[[auth-and-tokens]] for the two-credential split.

> **Why this type exists.** An earlier design copied the token into the event
> pump as a `String` at startup, where a refresh could never reach it — every
> metadata lookup would begin failing after an hour. See [[auth-and-tokens]].

### `SpotifySession`
Live librespot handles, present only while logged in:

| Field | Purpose |
| --- | --- |
| `session` | Kept alive for the login's lifetime. `#[allow(dead_code)]` — never read, but dropping it would end the connection |
| `spirc` | The transport handle every playback command goes through; owns the mixer internally, so volume goes through here too |
| `refresh_task` | `JoinHandle`, aborted on logout so the refresher cannot outlive the session |
| `remote_task` | `JoinHandle` for the remote-playback poller (Web API `GET /me/player`, gated to when not the active device), aborted alongside it |
| `connect_state_task` | `JoinHandle` for the event-first Connect cluster mirror (see [[remote_state.rs]]) — the Web API poller above is now only a recovery/metadata fallback for it |
| `friends_task` | `Option<JoinHandle>` for the dealer-driven friend-presence feed (see [[friends.rs]]); optional because this feature must never make an otherwise-healthy login fail |

Four background task handles now, not two — every one of them must be
explicitly `.abort()`ed on logout/session replacement, same reasoning as the
original two (see below).

### `ActiveDeviceSignal`
`tokio::sync::watch<bool>` wrapper — the **canonical, authoritative** signal
for whether this app is the active Connect device. `PlaybackState.is_active_device`
mirrors it for the webview snapshot, but this channel is the real source of
truth: written from exactly two places in `player::spawn_event_pump`
(`SessionConnected`/`Playing`/`Loading`/`Paused` → `true`,
`SessionDisconnected` → `false`, the latter fired by librespot's own Spirc via
a fully public `librespot_connect::spirc::handle_cluster_update` path — no
private endpoint is called to learn this). Anything that needs to *react* to
activation/deactivation rather than poll `AppState::playback` should
`subscribe()` here.

### `AppState`
```rust
spotify:               RwLock<Option<SpotifySession>>
playback:               RwLock<PlaybackState>
queue:                  RwLock<crate::queue::QueueView>
lyrics_cache:           RwLock<HashMap<String, crate::lyrics::LyricsResult>>
lyrics_requests:        Mutex<HashMap<String, Arc<Mutex<()>>>>
audio_capability_cache: RwLock<HashMap<String, crate::audio_capabilities::AudioCapability>>
friend_activity:        RwLock<crate::friends::FriendFeed>
saved_tracks_cache:     RwLock<Option<crate::library::SavedTracksSnapshot>>
telemetry:              crate::telemetry::TelemetryTracker
auth:                   RwLock<AuthState>
device_auth:            crate::auth::DeviceAuthStore
sleep_timer:            crate::sleep_timer::SleepTimerController
tokens:                 TokenStore
audio:                  AudioRuntime
active_device:          ActiveDeviceSignal
jams:                   RwLock<Option<Arc<crate::jams_bridge::JamController>>>
internal_spotify:       crate::spotify::InternalSpotify
```
Grew from 4 fields to 17 as each new feature landed its own cache or
controller here rather than threading state through `commands.rs` ad hoc.
Notable additions, each commented at the site in source:

- `queue` — last snapshot, hydrated first from Dealer then (when needed) by
  one Web API request for display metadata.
- `lyrics_cache` / `lyrics_requests` — the cache avoids a duplicate internal
  lyrics fetch for a URI the frontend already has; `lyrics_requests` is a
  per-track gate so Now Playing and a concurrent refresh can't double-fire.
- `audio_capability_cache` — per-session format/storage snapshots; storage
  URLs are never retained, only booleans/counts, so a long TTL is safe.
- `saved_tracks_cache` — session-local snapshot of the user's saved tracks so
  opening several artist pages in a row doesn't re-walk the whole library
  each time; invalidated on any save/unsave.
- `jams` — built lazily on the first jam command, dropped on logout. `Arc` so
  commands clone the handle instead of holding the read guard across awaits.
- `internal_spotify` — cached first-party clients (Pathfinder and friends);
  credentials are supplied per request and never kept here.

## Inputs / outputs / side effects

Pure data definitions plus the `PlaybackState`/`ActiveDeviceSignal` methods
above. All serialisable types use `serde(rename_all = "camelCase")` to match
[[types.ts]].

## Dependencies

**Imports:** `librespot::connect::Spirc`, `librespot::core::session::Session`,
`serde`, `tokio::sync::{watch, Mutex, RwLock}`, [[audio-mod]] (`AudioRuntime`,
`StreamQuality`)
**Imported by:** [[lib.rs]], [[commands.rs]], [[player.rs]], [[auth.rs]],
[[media_keys.rs]], [[friends.rs]], [[jams_bridge.rs]], [[remote_state.rs]],
and every other feature module that owns a cache on `AppState`

## Notable logic / gotchas

- **`Option<SpotifySession>` makes "logged in" structural.** When it is `None`
  there is no `Spirc` to call, so [[commands.rs]]'s `with_spirc` helper cannot
  act on a dead session — the type system enforces it rather than a boolean.
- **`tokens` sits outside `SpotifySession`** so reading the token does not
  require locking the whole session. Important because the event pump reads it
  on every track change.
- **All four `SpotifySession` `JoinHandle`s must be aborted explicitly.**
  Dropping one *detaches* the task rather than cancelling it. Forgetting this
  for `refresh_task` once left two refreshers hitting the token endpoint on
  independent schedules — see [[rate-limiting]]. The same hazard applies to
  `remote_task`, `connect_state_task`, and `friends_task`.
- **`tokio::sync::RwLock`, not `std::sync`** — these are held across `.await`
  points; a `std` lock would deadlock the runtime.
- **`ActiveDeviceSignal` vs. `PlaybackState.is_active_device`** — the watch
  channel is authoritative and is what `load_context`/`load_tracks`/
  `activate_this_device` check before acting (see the "registering ≠
  activating" note in `CLAUDE.md`); the `PlaybackState` field is a read-only
  mirror for the frontend snapshot. Writing the field directly without going
  through `ActiveDeviceSignal::set` would desync the two.
- `AppState` derives `Default`, which is what `AppState::new()` returns —
  everything starts empty and logged out.

## See also

[[state-and-events]] · [[auth-and-tokens]] · [[types.ts]] · [[player.rs]] ·
[[commands.rs]] · [[error.rs]] · [[friends.rs]] · [[remote_state.rs]] ·
[[jams_bridge.rs]] · [[backend-rust]] · [[MOC]]
