---
tags: [file, backend, playback, rust]
---
# `src-tauri/src/player.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **330 lines**

The most intricate file in the project. Owns the librespot audio pipeline, the
Connect device identity, and the event pump that drives the entire UI.
Conceptual overview: [[playback-and-connect]] and [[state-and-events]].

## Key items

### `const MAX_VOLUME: u16 = u16::MAX`
### `fn percent_to_volume(percent: u8) -> u16`
Converts the UI's `0..=100` to librespot's `0..=65535`. **Always use this** —
see the gotcha below.

### `async fn start_session(...) -> StartedSession`
Builds and starts everything. Order is significant:

1. Configs: `SessionConfig`, `PlayerConfig`, `AudioFormat`, `MixerConfig`.
2. `ConnectConfig` — device name, `DeviceType::Computer`,
   `initial_volume: percent_to_volume(50)`.
3. `Cache::new(...)` — credentials + audio, capped at **2 GB**.
4. `audio_backend::find(None)` → rodio → WASAPI on Windows.
5. `mixer::find(None)` → softvol mixer.
6. `Session::new`, then `Player::new`.
7. **`player.get_player_event_channel()`** — must happen here.
8. `Spirc::new(connect_config, session, credentials, player, mixer)`.
9. Spawn `spirc_task` (the Connect protocol loop).
10. `spawn_event_pump(...)`.
11. `spirc.activate()`.
12. Seed `playback.volume` from `mixer.volume()`.

### `struct StartedSession { session, spirc }`
What is handed back; [[commands.rs]] pairs it with the refresher handle to
build a `SpotifySession`.

### `fn spawn_event_pump(app, rx, tokens)`
The only writer of `AppState.playback`. Consumes librespot's `PlayerEvent`
stream, mutates the snapshot, resolves metadata, and emits
`playback:changed`. Unhandled variants `continue` without emitting.

### `async fn fetch_track_info(api, token, uri) -> TrackInfo`
Branches on `uri.item_type()`: `"episode"` → `/v1/episodes/{id}`, everything
else → `/v1/tracks/{id}`.

### `fn pick_cover(images) -> Option<String>`
Chooses the image closest to **300px** rather than the largest, so a low-end
CPU is not decoding a 640px JPEG for a 52px thumbnail.

### `async fn snapshot(state) -> PlaybackState`
Clones the current state; backs the `get_playback` command.

## Inputs / outputs / side effects

- **Audio output** via rodio/WASAPI — the app's core side effect.
- **Network:** librespot's Spotify connection; Web API metadata lookups.
- **Filesystem:** librespot cache under `<app data>/cache`.
- **Spawns** two long-lived tasks: `spirc_task` and the event pump.
- **Emits** `playback:changed` to the webview.

## Dependencies

**Imports:** `librespot::{connect, core, playback}`, `tauri`, `tokio`, `serde`,
[[state.rs]], [[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]]

## Notable logic / gotchas

> ### 1. `get_player_event_channel()` before the move
> `Spirc::new` **takes ownership** of `player`. The event channel must be
> obtained beforehand or there is no way to observe playback at all. This
> single ordering constraint is what makes the whole UI work.

> ### 2. `initial_volume` is raw, not a percentage
> The field is on the `0..=65535` scale; librespot's own default is
> `u16::MAX / 2`. Passing `50` gives ~0.08% volume — audible silence that looks
> exactly like a broken audio backend. librespot's doc comment reads
> `(default: 50%)`, describing *intent*, not the literal. A source comment
> marks this.

- **Metadata comes from the Web API, not `AudioItem`.** librespot's covers are
  raw file IDs needing URL construction; the Web API returns ready-to-use CDN
  URLs and a documented shape. Cost: one HTTP request per *new* track, cached
  by URI in a `HashMap` for the session.
- **A metadata failure is non-fatal** — logs a warning and leaves the previous
  track displayed rather than blanking the bar.
- **The pump reads the token through `TokenStore`**, so refreshes are picked up
  automatically. Passing a `String` here was a real bug.
- **`is_active_device` is inferred** from `Playing`/`Paused`/`Loading` vs.
  `SessionDisconnected`; librespot exposes no explicit flag.
- **`spirc_task` must keep running.** Drop it and the device disappears from
  Spotify Connect.
- **The 2 GB cache cap** is explicit; librespot would otherwise grow unbounded.
- The `_ =>` arm `drop(pb)`s the write guard before `continue`, so no snapshot
  is emitted for events the UI does not render.

## See also

[[playback-and-connect]] · [[state-and-events]] · [[data-flow]] ·
[[commands.rs]] · [[state.rs]] · [[webapi.rs]] · [[auth.rs]] ·
[[known-limitations]] · [[backend-rust]] · [[MOC]]
