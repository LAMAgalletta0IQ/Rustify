---
tags: [concept, state]
---
# State and events

How backend state reaches the UI.

## `AppState` — the single managed object

Registered once via `.manage()` in [[lib.rs]] and reachable from any command or
task through `app.state::<AppState>()`. Defined in [[state.rs]]:

| Field | Type | Holds |
| --- | --- | --- |
| `spotify` | `RwLock<Option<SpotifySession>>` | Live librespot handles. `None` = logged out |
| `playback` | `RwLock<PlaybackState>` | The snapshot pushed to the UI |
| `auth` | `RwLock<AuthState>` | Profile info |
| `tokens` | `TokenStore` | Web API bearer token |

All `tokio::sync` locks, not `std` — they are held across `.await` points.

`Option<SpotifySession>` is what makes "logged in" a *structural* fact rather
than a boolean: if it is `None` there is no `Spirc` to call, so
[[commands.rs]]'s `with_spirc` helper cannot accidentally act on a dead session.

### Why `tokens` sits outside `SpotifySession`

`TokenStore` is `Arc<RwLock<String>>` and is cloned into the event pump and the
refresher. Keeping it out of `SpotifySession` means reading the token does not
require locking the whole session — and, critically, that a refresh is visible
to every holder at once. See [[auth-and-tokens]].

## `PlaybackState` — one flat snapshot

```rust
is_playing, is_loading, is_active_device,
track: Option<TrackInfo>,
position_ms, duration_ms, volume,
shuffle, repeat_context, repeat_track
```

Deliberately flat and cheap to clone: it is serialised on every position
correction, and the target hardware is a low-end CPU. It is sent whole rather
than as a diff — simpler, and small enough that diffing would cost more than it
saves.

`volume` is librespot's raw `0..=65535`. The UI converts with `volumeToPercent`
in [[types.ts]]. **This scale is a live hazard** — see [[player.rs]] for the
`initial_volume` bug it caused.

## The event pump

`spawn_event_pump` in [[player.rs]] is the only writer of `playback`. It
consumes librespot's `PlayerEvent` stream (21 variants) and maps the ones the
UI renders:

| `PlayerEvent` | Effect |
| --- | --- |
| `Playing` / `Paused` | `is_playing`, position, mark active device, resolve track |
| `Loading` | `is_loading`, resolve track |
| `Stopped` | Clear playing/loading, reset position |
| `PositionCorrection` / `PositionChanged` / `Seeked` | Update position |
| `VolumeChanged` | Update volume |
| `ShuffleChanged` / `RepeatChanged` | Update modes |
| `Unavailable` | Clear loading |
| `SessionDisconnected` | Clear active-device flag |
| everything else | `continue` — no snapshot emitted |

That final `continue` matters: `Preloading`, `EndOfTrack` and
`TimeToPreloadNextTrack` fire often and carry nothing renderable, so skipping
them avoids pointless IPC traffic.

### Metadata resolution

On a track change the pump fetches `/v1/tracks/{id}` (or `/v1/episodes/{id}`)
and caches by URI. Chosen over librespot's `AudioItem` because the Web API
returns ready-to-use CDN cover URLs, where librespot returns raw file IDs.

Cost: one HTTP request per *new* track. A metadata failure logs a warning and
leaves the previous track shown rather than blanking the bar.

## Emitting to the frontend

Event names live in `state::events` ([[state.rs]]) and are duplicated as string
constants in [[api.ts]]. **Nothing enforces that they match** — a typo silently
stops UI updates.

| Constant | Name | Payload |
| --- | --- | --- |
| `events::PLAYBACK` | `playback:changed` | `PlaybackState` |
| `events::AUTH` | `auth:changed` | `AuthState` |

## Receiving in Svelte

[[store.svelte.ts]] holds `playback` and `auth` as `$state` runes. Components
read `store.playback.*` and re-render automatically.

Two details worth knowing:

**The local ticker.** The backend only sends position updates periodically. The
store runs a 1 Hz `setInterval` while playing so the progress bar moves
smoothly. 1 Hz rather than `requestAnimationFrame` is a deliberate CPU
trade-off. It starts and stops with `isPlaying`, and clamps to `durationMs` so
it cannot run past the end.

**`store.run()`.** Wraps a backend call, clearing `error` first and populating
it on failure, so any component gets the error banner for one line of code.

## `is_active_device` is inferred, not read

librespot exposes no explicit "am I the active device" flag. The pump infers
it: `true` on `Playing`/`Paused`/`Loading`, `false` on `SessionDisconnected`.

Good enough in practice, but it is a heuristic — if you transfer playback away
and the flag looks wrong, this is why. Listed in [[known-limitations]].

## See also

[[architecture]] · [[data-flow]] · [[auth-and-tokens]] · [[state.rs]] ·
[[player.rs]] · [[store.svelte.ts]] · [[types.ts]] · [[MOC]]
