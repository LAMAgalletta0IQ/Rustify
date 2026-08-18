---
tags: [file, backend, rust]
---
# `src-tauri/src/commands.rs`

**Module:** [[backend-rust]] · **Language:** Rust

## Purpose

The complete IPC surface — every `#[tauri::command]` function. This is the
*only* backend code the frontend can reach. It holds no domain logic: each
command unwraps state, delegates to a feature module, and lets errors
propagate.

## Helpers

### `async fn token(state) -> AppResult<String>`
Checks a session exists, then reads the bearer token from `TokenStore`. Every
Web API command starts here, so "not logged in" is one uniform error.

### `async fn with_spirc<F>(state, f) -> AppResult<()>`
Takes a read lock, errors with `NotLoggedIn` if the session is `None`, and runs
the closure against the live `Spirc`. Every playback command funnels through
this, so none can act on a dead session.

### `fn device_name() -> String`
`<COMPUTERNAME> (Rustify)`, falling back to `"Rustify"`. This is the
name that appears in Spotify's device list.

### `fn get_login_info(app) -> AppResult<LoginInfo>`
Reports `privateClientId`, `clientIdEnv` and `webapiRedirectUri`. `privateClientId`
now carries real meaning — whether `auth::webapi_client_id` actually resolves —
rather than being hardcoded `true`; both [[Setup.svelte]] (to know when it can
hand off to Login) and [[Login.svelte]] (for its copy) call this on mount.

### `fn set_client_id(app, client_id) -> AppResult<()>`
Trims and rejects an empty string, then writes it to `settings.json` via
`auth::save_settings`. The only way a Client ID reaches disk outside the
`RUSTIFY_CLIENT_ID` env override; called by [[Setup.svelte]]'s save button.

### `async fn establish(app, state, api, toks) -> AuthState`
Shared by `login` and `restore_session`. Takes a `SessionTokens` (both logins).
**Order is load-bearing:**

1. Resolve the app data dir.
2. **Persist both refresh tokens** — before the Premium gate.
3. **Premium check** ([[auth.rs]]) using the *Web API* token.
4. `tokens.set(webapi access_token)` — publish before anything reads it.
5. `player::start_session(...)` with the *streaming* token, the one carrying
   the `streaming` scope.
6. `spawn_refresher(...)` under the Web API client ID.
7. `spawn_remote_poller(...)`.
8. **Tear down any existing session** (aborting both its tasks), then store the
   new `SpotifySession`, update `auth`, emit `auth:changed`.

Steps 3–5 are where the two tokens diverge: mixing them up yields a 403 on
library calls or a failure to stream.

> **Why the token is saved before the gate.** OAuth has already succeeded by
> step 2. If the Premium check then fails transiently — a 429 on `/me` is the
> realistic case — a retry can go through `restore_session` silently instead of
> reopening the browser. Saving after the gate meant a rate-limited login left
> nothing stored, so every retry reopened a browser tab. See [[rate-limiting]].

## Command groups

### Auth
| Command | Notes |
| --- | --- |
| `get_auth_state` | Clone of current `AuthState` |
| `get_login_info` | Shape of the login flow, and whether Setup is still needed |
| `set_client_id` | Saves the Web API Client ID from [[Setup.svelte]] to `settings.json` |
| `login` | Interactive; opens the browser **twice**. Errors if no Client ID is configured, though the UI should never let this be reached |
| `restore_session` | Silent. **Returns a logged-out state rather than erroring** when no Client ID is configured yet, nothing is stored, or the grant is rejected — the UI just shows Setup or Login. Deletes `tokens.json` **only** on `auth::is_grant_rejected`; a transient failure keeps them |
| `logout` | Shuts down Spirc, **aborts both background tasks**, clears token and state, deletes `tokens.json`, emits `auth:changed` |

### Playback
`play`, `pause`, `play_pause`, `next_track`, `previous_track`, `seek`,
`set_volume`, `set_shuffle`, `set_repeat`, `load_context`, `load_tracks`,
`get_playback`.

- `set_volume` takes `percent: u8` and converts via `percent_to_volume`.
- `set_repeat` issues **two** Spirc calls (`repeat` then `repeat_track`);
  librespot models context-repeat and track-repeat separately.
- `load_context(context_uri, track_uri?)` — plays a container starting at a
  track. `load_tracks(uris, start_uri?)` — ad-hoc list, returns `Ok(())` early
  on an empty list. Both set `start_playing: true`.
- **Both load commands activate this device first**, since [[player.rs]] no
  longer activates at login. Each reads `is_active_device` beforehand and skips
  the call when already active, avoiding librespot's
  `SpircCommand::Activate will be ignored while already active`.
- **These control this app's player only.** While another device is active the
  UI mirrors its state via the poller, but these buttons do not command it —
  Spotify wants `PUT /me/player/play` for that. See [[known-limitations]].

### Connect
`list_devices`, `transfer_playback` (Web API), `activate_this_device`
(librespot `spirc.activate()`). Mixed mechanisms — see
[[playback-and-connect]].

### Library
`get_playlists`, `get_playlist_tracks`, `get_saved_tracks`, `get_saved_albums`,
`get_album_tracks`, `set_tracks_saved`, `set_albums_saved`, `get_tracks_saved`,
`get_artist_top_tracks`, `get_artist_albums`. Paginated commands take
`Option<u32>` limit/offset with sane defaults.

### Search / Queue
`search_spotify`, `get_queue`, `add_to_queue`.

## Inputs / outputs / side effects

Inputs arrive from the webview as camelCase JSON (Tauri converts to snake_case
parameters). Outputs serialise back as camelCase. Side effects are delegated —
network, audio, filesystem all happen in the modules called.

## Dependencies

**Imports:** `librespot::connect::{LoadRequest, LoadRequestOptions,
PlayingTrack}`, `librespot::core::authentication::Credentials`, `tauri`,
[[auth.rs]], [[connect.rs]], [[library.rs]], [[player.rs]], [[queue.rs]],
[[search.rs]], [[state.rs]], [[webapi.rs]], [[error.rs]]
**Imported by:** [[lib.rs]] (registration only)

## Notable logic / gotchas

- **Adding a command here is not enough.** It must also be listed in
  `generate_handler!` in [[lib.rs]] and wrapped in [[api.ts]]. Missing either
  fails at runtime, not compile time.
- **Parameter names are part of the contract.** Tauri maps camelCase JSON keys
  to snake_case parameters, so `contextUri` in [[api.ts]] must pair with
  `context_uri` here. A mismatch yields a `null` argument, not an error.
- **`start_playing: true` on both load commands.** The default is `false`;
  relying on it and calling `play()` afterwards is an extra round trip and a
  race.
- **`logout` aborts the refresh task.** Forgetting this would leak a task that
  keeps refreshing tokens for a session that no longer exists.
- **`establish` tears the old session down too, for the same reason.**
  **Dropping a tokio `JoinHandle` detaches the task rather than cancelling it**,
  so replacing `state.spotify` without an explicit `abort()` left the previous
  refresher running. Two refreshers then hit the token endpoint on independent
  schedules — a way to rate-limit yourself. This was a real bug; see
  [[rate-limiting]].
- **`establish` logs which token halves actually persisted**, at info:
  `persisting tokens: streaming=<bool>, web api=<bool> (split=<bool>)`, where
  `split` is whether the Web API client ID differs from the streaming one.
  Read this line first when diagnosing quota problems. Without it, a Web API
  refresh token that never reaches disk is invisible until the *next* launch
  silently falls back to the shared quota and starts collecting 429s — the
  failure and its cause are separated by a restart. See [[auth.rs]].
- **`probe_webapi` is gone.** It was a temporary diagnostic that took an
  arbitrary path, `GET`-ed it and returned the first 300 characters, used to
  map which endpoints Spotify was refusing. Removing a command means deleting
  it from `generate_handler!` in [[lib.rs]] as well; leaving the registration
  behind is a *compile* error, which is the one direction of this contract the
  compiler does check.
- **A new `WebApi::new()` per command** builds a fresh `reqwest::Client` each
  call. Slightly wasteful — reqwest clients are designed to be reused for
  connection pooling — but keeps commands stateless. A reasonable future
  optimisation is to hold one in `AppState`.

## See also

[[lib.rs]] · [[api.ts]] · [[state.rs]] · [[auth.rs]] · [[player.rs]] ·
[[error.rs]] · [[rate-limiting]] · [[data-flow]] · [[architecture]] ·
[[Setup.svelte]] · [[backend-rust]] · [[MOC]]
