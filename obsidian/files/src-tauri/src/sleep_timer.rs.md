---
tags: [file, backend, player, rust]
---
# `src-tauri/src/sleep_timer.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **347 lines**

Cancellable sleep timer driven by monotonic time and real player events —
duration mode and end-of-track mode, both cancellable, both reflected live
in the UI via a countdown.

## Key items

### `struct SleepTimerController`
Owned by [[state.rs]]'s `AppState`. `start(seconds)`, `sleep_at_end_of_track()`,
`cancel()`, current `SleepTimerStatus` (active/mode/`endsAtUnixMs`/
`remainingSeconds`).

### Remote-device end-of-track
For an active *remote* device (this app not the active Connect device), the
end-of-track deadline is **estimated** from the last known track duration/
position rather than a real event, since the Web API can't push a private
end-of-track signal — the UI is expected to treat this as an estimate, not
an exact countdown.

## Dependencies

**Imports:** `tokio::time`, [[player.rs]] event hooks
**Imported by:** [[commands.rs]] (`start_sleep_timer`, `sleep_at_end_of_track`,
`cancel_sleep_timer`, `get_sleep_timer`), [[state.rs]]

## See also

[[commands.rs]] · [[NowPlaying.svelte]] (the sleep-timer menu) · [[MOC]]
