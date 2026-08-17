---
tags: [file, backend, rust]
---
# `src-tauri/src/commands.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **424 lines**

## Purpose

The complete IPC surface — all 32 `#[tauri::command]` functions. This is the
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
`<COMPUTERNAME> (spotify-rust)`, falling back to `"spotify-rust"`. This is the
name that appears in Spotify's device list.

### `async fn establish(app, state, api, tok) -> AuthState`
Shared by `login` and `restore_session`. Order matters:
1. **Premium check first** ([[auth.rs]]) — fail before starting audio.
2. Resolve the app data dir.
3. `tokens.set(access_token)` — publish *before* anything reads it.
4. `player::start_session(...)`.
5. Persist the refresh token.
6. `spawn_refresher(...)`.
7. Store `SpotifySession`, update `auth`, emit `auth:changed`.

## Command groups

### Auth
| Command | Notes |
| --- | --- |
| `get_auth_state` | Clone of current `AuthState` |
| `login` | Interactive; opens the browser |
| `restore_session` | Silent. **Returns a logged-out state rather than erroring** when nothing is stored or the token is rejected — the UI just shows the login screen |
| `logout` | Shuts down Spirc, **aborts the refresh task**, clears token and state, deletes `tokens.json`, emits `auth:changed` |

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
- **A new `WebApi::new()` per command** builds a fresh `reqwest::Client` each
  call. Slightly wasteful — reqwest clients are designed to be reused for
  connection pooling — but keeps commands stateless. A reasonable future
  optimisation is to hold one in `AppState`.

## See also

[[lib.rs]] · [[api.ts]] · [[state.rs]] · [[auth.rs]] · [[player.rs]] ·
[[error.rs]] · [[data-flow]] · [[architecture]] · [[backend-rust]] · [[MOC]]
