---
tags: [file, backend, rust]
---
# `src-tauri/src/commands.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~1792 lines, 82 commands**

> Until 2026-08 this counted 43 commands and `establish` spawned only two
> background tasks (refresher, remote poller). It now spawns four —
> `connect_state_task` ([[remote_state.rs]]) and `friends_task`
> ([[friends.rs]]) joined the refresher and remote poller — and whole new
> command groups (DJ/Home, sleep timer, credits, episode resume, telemetry,
> music video, audio capability, friend activity, profiles, jams) landed.
> This note's "Command groups" section and `establish()` walkthrough are
> updated to match; the false claim that `get_artist_top_tracks` still
> exists has been removed — it was replaced by `get_artist_overview`.

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

### `async fn remote_put(state, path, query)` / `async fn remote_post(state, path, query)`
Thin wrappers issuing an authenticated `PUT`/`POST` against SpClient/Web API
paths outside the typed [[webapi.rs]] helpers — used by the DJ/Home and
episode-completion commands, which need one-off calls that don't warrant a
whole typed module.

### `async fn clear_crossfade_before_transition(app, state) -> AppResult<bool>`
Temporarily zeroes the configured crossfade before a track transition that
must be a hard cut (e.g. an explicit skip during DJ narration) and reports
whether it changed anything, so the caller knows whether to restore it
afterward.

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
   `connection_status` moves to `Connecting`.
5. `player::start_session(...)` with the *streaming* token, the one carrying
   the `streaming` scope, and a `PlaybackOptions` built from `settings`
   (volume, cache limit, quality, crossfade).
6. `spawn_refresher(...)` under the Web API client ID — access tokens last
   ~1h, so without this every Web API call would start failing mid-session.
7. **Tear down any existing session first** — `spirc.shutdown()` plus
   `.abort()` on **all four** background tasks (`refresh_task`,
   `remote_task`, `connect_state_task`, and `friends_task` if present).
   Dropping a `JoinHandle` only detaches it; a second `establish` without a
   logout would otherwise leave the old refresher running, and two
   refreshers hitting the token endpoint on independent schedules is exactly
   how a client rate-limits itself.
8. `spawn_remote_poller(...)` (fallback path) and
   `crate::remote_state::spawn(...)` (primary Dealer-driven Connect mirror,
   see [[remote_state.rs]]) — **if the latter fails**, both the remote poller
   and the refresher are aborted, `spirc.shutdown()` is called, connection
   status drops back to `Disconnected`, and `establish` returns `Err`; a
   working Connect mirror is treated as load-bearing, not optional.
9. Reset `friend_activity` to a fresh default, then
   `crate::friends::spawn(...)` — **if this fails, `establish` does not
   fail**. Friend presence is an optional capability: the error is logged at
   debug and `friend_activity` is set to `FriendFeedStatus::Failed` (a
   registration failure is a connectivity problem, not evidence Spotify
   refused the capability, which only a genuine 403/404 mid-session would
   mean — see [[friends.rs]]'s `is_capability_absent`).
10. `connection_status` moves to `Connected`, the new `SpotifySession` is
    stored (all four task handles included), `auth` is updated, and
    `auth:changed` is emitted.

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
| `get_settings`, `update_settings` | Validated, merged functional settings (volume, motion, cache) |
| `login` | Interactive; opens the browser **twice**. Errors if no Client ID is configured, though the UI should never let this be reached |
| `restore_session` | Reuses a live Rust session after webview reload; otherwise silently restores stored credentials. Deletes `tokens.json` only on a rejected grant |
| `logout` | Cancels device-auth/sleep-timer, checkpoints an in-progress episode's resume position ([[podcasts.rs]]), shuts down Spirc, **aborts all four `SpotifySession` background tasks**, drops the jam controller ([[jams_bridge.rs]]), clears every cache (`queue`, `lyrics_cache`, `lyrics_requests`, `audio_capability_cache`, `friend_activity`) and token/auth state, deletes `tokens.json`, emits `auth:changed` |

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
`get_playlists`, `get_playlist_tracks` (retries via
`internal_spotify.playlist_contents` — [[spotify/playlist_contents.rs]] —
on an exact `AppError::Unavailable`/404, which is what a generated-playlist
id like Daily Mix returns from the public REST path),
`get_saved_tracks`, `get_saved_albums`, `get_followed_artists`,
`get_followed_releases` ([[library.rs]]'s `followed_releases`, backing
[[Releases.svelte]]), `get_recently_played`, `get_quick_access`,
`record_relevance` (writes to [[relevance.rs]]'s local ranking file),
`get_album_tracks`, `set_tracks_saved`, `set_albums_saved`,
`set_artists_saved`, `get_tracks_saved`, `get_albums_saved`,
`get_artists_saved`, `get_liked_tracks_by_artist` (served from
[[library.rs]]'s session-local saved-tracks cache), `get_artist_albums`,
`get_artist`, `get_artist_overview` (replaces the old
`get_artist_top_tracks`/`get_artist_concerts` pair — one Pathfinder
`queryArtistOverview` call via [[spotify/artist_extras.rs]] covers stats +
top tracks + concerts together; falls back to [[library.rs]]'s
`artist_tracks` REST reconstruction only if the Pathfinder response's
top-tracks field comes back empty), `get_track_credits`
([[spotify/credits.rs]]), `get_top_tracks`, `get_top_artists`. Paginated
commands take `Option<u32>` limit/offset with sane defaults.

### DJ / Home
`get_personalized_home` ([[spotify/home.rs]]), `get_dj_status`, `start_dj`
— DJ X context resolution and (partial) narration metadata; music playback
works end-to-end, narration audio does not (see [[known-limitations]] and
[[spotify/dj.rs]]).

### Sleep timer
`get_sleep_timer`, `start_sleep_timer`, `sleep_at_end_of_track`,
`cancel_sleep_timer` — thin wrappers over [[sleep_timer.rs]]'s controller.

### Episode / podcast, music video, audio capability, telemetry
`get_episode_resume`, `set_episode_completed` ([[podcasts.rs]]'s Herodotus
resume state), `get_music_video_capability` ([[music_videos.rs]]),
`get_audio_capability` ([[audio_capabilities.rs]], cached per-session in
`AppState.audio_capability_cache`), `get_telemetry_status`
([[telemetry.rs]]'s bounded local playback ledger — no first-party-
impersonating sender).

### Social — friend activity, profiles
`get_friend_activity` (reads `AppState.friend_activity`, kept live by
[[friends.rs]]'s dealer-driven `friends_task`), `get_user_profile`
([[profiles.rs]]'s rich SpClient profile), `search_users`
([[spotify/users.rs]]'s fuzzy Pathfinder search, backing [[Profile.svelte]]'s
user search).

### Lyrics
`get_lyrics` requires a live session and delegates read-only lookup/parsing to
[[mod.rs|lyrics/mod.rs]].

### Search / Queue
`search_spotify`, `get_queue`, `add_to_queue`.

### Jams (experimental)
`get_jam_status`, `create_jam`, `refresh_jam`, `join_jam`, `leave_jam`,
`add_track_to_jam`, `set_jam_queue_control`, `kick_jam_member`, `end_jam` —
all funnel through the private `ensure_jams` helper, which lazily builds the
one `JamController` held in `AppState.jams` on first use (see
[[jams_bridge.rs]] for how it's wired to the real session and
[[jams/mod.rs]] for the self-contained module underneath).

## Inputs / outputs / side effects

Inputs arrive from the webview as camelCase JSON (Tauri converts to snake_case
parameters). Outputs serialise back as camelCase. Side effects are delegated —
network, audio, filesystem all happen in the modules called.

## Dependencies

**Imports:** `librespot::connect::{LoadRequest, LoadRequestOptions,
PlayingTrack}`, `librespot::core::authentication::Credentials`, `tauri`,
[[auth.rs]], [[connect.rs]], [[library.rs]], [[player.rs]], [[queue.rs]],
[[search.rs]], [[state.rs]], [[webapi.rs]], [[error.rs]], [[remote_state.rs]],
[[friends.rs]], [[profiles.rs]], [[podcasts.rs]], [[music_videos.rs]],
[[audio_capabilities.rs]], [[telemetry.rs]], [[sleep_timer.rs]],
[[relevance.rs]], [[jams_bridge.rs]], `crate::spotify` ([[spotify/mod.rs]]
and its submodules)
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
- **`logout` aborts all four `SpotifySession` tasks.** Forgetting any one of
  them would leak a task hitting the token endpoint, polling `/me/player`, or
  holding a Dealer subscription open for a session that no longer exists.
- **`establish` tears the old session down too, for the same reason.**
  **Dropping a tokio `JoinHandle` detaches the task rather than cancelling it**,
  so replacing `state.spotify` without an explicit `abort()` left the previous
  refresher running. Two refreshers then hit the token endpoint on independent
  schedules — a way to rate-limit yourself. This was a real bug; see
  [[rate-limiting]].
- **`connect_state_task` failing aborts the whole login; `friends_task`
  failing does not.** A working Connect-cluster mirror is treated as
  load-bearing (without it, remote-device state has no primary source);
  friend presence is explicitly optional and degrades to
  `FriendFeedStatus::Failed` instead. See `establish`'s walkthrough above.
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
[[remote_state.rs]] · [[friends.rs]] · [[jams_bridge.rs]] · [[error.rs]] ·
[[rate-limiting]] · [[data-flow]] · [[architecture]] · [[Setup.svelte]] ·
[[backend-rust]] · [[MOC]]
