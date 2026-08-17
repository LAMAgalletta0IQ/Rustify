---
tags: [file, backend, state, rust]
---
# `src-tauri/src/state.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **100 lines**

## Purpose

Defines all shared application state and the data shapes sent to the webview.
A leaf module: everything depends on it, it depends on nothing in the crate.
The conceptual overview is in [[state-and-events]].

## Key items

### `mod events`
```rust
pub const PLAYBACK: &str = "playback:changed";
pub const AUTH: &str = "auth:changed";
```
Tauri event names. **Must match the constants in [[api.ts]]** — nothing checks
this, and a mismatch silently stops UI updates.

### `TrackInfo`
Flattened now-playing metadata: `uri`, `name`, `artists: Vec<String>`,
`album`, `cover_url: Option<String>`, `duration_ms`. Populated by the Web API
lookup in [[player.rs]], not by librespot.

### `PlaybackState`
The single snapshot pushed to the UI:

| Field | Notes |
| --- | --- |
| `is_playing`, `is_loading` | Transport |
| `is_active_device` | **Inferred**, not read — see [[known-limitations]] |
| `track` | `Option<TrackInfo>` |
| `position_ms`, `duration_ms` | |
| `volume` | **Raw `0..=65535`**, librespot's scale |
| `shuffle`, `repeat_context`, `repeat_track` | |

Deliberately flat and cheap to clone — serialised on every position correction,
and the target hardware is a low-end CPU.

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
refresh is visible to every holder at once.

> **Why this type exists.** An earlier design copied the token into the event
> pump as a `String` at startup, where a refresh could never reach it — every
> metadata lookup would begin failing after an hour. See [[auth-and-tokens]].

### `SpotifySession`
Live librespot handles, present only while logged in:

| Field | Purpose |
| --- | --- |
| `session` | Kept alive for the login's lifetime. `#[allow(dead_code)]` — never read, but dropping it would end the connection |
| `spirc` | The transport handle every playback command goes through |
| `refresh_task` | `JoinHandle`, aborted on logout so the refresher cannot outlive the session |

### `AppState`
```rust
spotify:  RwLock<Option<SpotifySession>>
playback: RwLock<PlaybackState>
auth:     RwLock<AuthState>
tokens:   TokenStore
```

## Inputs / outputs / side effects

Pure data definitions. No I/O. All serialisable types use
`serde(rename_all = "camelCase")` to match [[types.ts]].

## Dependencies

**Imports:** `librespot::connect::Spirc`, `librespot::core::session::Session`,
`serde`, `tokio::sync::RwLock`, `tauri::async_runtime::JoinHandle`
**Imported by:** [[lib.rs]], [[commands.rs]], [[player.rs]], [[auth.rs]],
[[media_keys.rs]]

## Notable logic / gotchas

- **`Option<SpotifySession>` makes "logged in" structural.** When it is `None`
  there is no `Spirc` to call, so [[commands.rs]]'s `with_spirc` helper cannot
  act on a dead session — the type system enforces it rather than a boolean.
- **`tokens` sits outside `SpotifySession`** so reading the token does not
  require locking the whole session. Important because the event pump reads it
  on every track change.
- **`tokio::sync::RwLock`, not `std::sync`** — these are held across `.await`
  points; a `std` lock would deadlock the runtime.
- `AppState` derives `Default`, which is what `AppState::new()` returns —
  everything starts empty and logged out.

## See also

[[state-and-events]] · [[auth-and-tokens]] · [[types.ts]] · [[player.rs]] ·
[[commands.rs]] · [[error.rs]] · [[backend-rust]] · [[MOC]]
