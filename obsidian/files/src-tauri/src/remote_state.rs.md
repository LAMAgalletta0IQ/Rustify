---
tags: [file, backend, dealer, connect, rust]
---
# `src-tauri/src/remote_state.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **444 lines** · added 2026-08

Event-first Spotify Connect state projection. librespot 0.8 consumes the
Connect cluster protobuf internally but doesn't expose it; Dealer supports
fan-out subscriptions, so this observes the **same** message Spirc itself
consumes and projects only UI-safe state from it — the application-level
equivalent of librespot PR #1704's later watch channels, without carrying a
vendored patch. This is now the **primary** source for remote (another
device's) playback/queue state; [[player.rs]]'s `spawn_remote_poller` (5 s
`GET /me/player`) is a recovery/metadata fallback for whatever the event
stream doesn't cover, not the main path anymore.

## Key items

### `pub fn spawn(app, session) -> AppResult<JoinHandle<()>>`
Subscribes to the cluster/player protobuf over Dealer, projects device
list, active device, playback snapshot and queue on every push.

### Reconnect/backoff
Re-subscribes on Dealer reconnect rather than assuming the original
subscription survives; reconciles the active-device key against what the
cluster reports, since a stale local guess (e.g. after this device was the
active one and lost that status) must not linger.

### Stale-state cleanup
When account playback ends entirely (no device active anywhere), clears
local state rather than leaving the last-known track displayed
indefinitely.

## Dependencies

**Imports:** `librespot::core::session::Session`, [[state.rs]], [[queue.rs]]
**Imported by:** [[commands.rs]] (spawned alongside the other session tasks
at login), [[state.rs]] (`connect_state_task`)

## Notable logic / gotchas

> ### Two writers, reconciled on purpose
> [[player.rs]]'s event pump describes only audio *this app* produces; this
> file describes what Dealer says the whole account's Connect topology is
> doing. Both write into the same [[state.rs]] `PlaybackState`/`QueueView` —
> see [[state-and-events]] for the anchoring rules that keep them from
> fighting over position/track fields.

## See also

[[player.rs]] · [[state.rs]] · [[queue.rs]] · [[state-and-events]] ·
[[playback-and-connect]] · [[MOC]]
