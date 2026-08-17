---
tags: [file, backend, playback, rust]
---
# `src-tauri/src/media_keys.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **62 lines**

## Purpose

Registers the OS media keys (Play/Pause, Next, Previous) so transport control
works while the window is unfocused or minimised.

Registered **from Rust rather than the webview** — that is the whole point. A
webview key handler only fires when the window has focus.

## Key items

### `const KEYS: [(Code, Action); 3]`
```rust
(Code::MediaPlayPause,      Action::PlayPause)
(Code::MediaTrackNext,      Action::Next)
(Code::MediaTrackPrevious,  Action::Prev)
```

### `enum Action { PlayPause, Next, Prev }`
A small indirection so one closure body serves all three keys.

### `pub fn register(app: &AppHandle)`
For each key: build a `Shortcut::new(None, code)` (no modifiers) and register a
handler that

1. **Returns early unless `event.state() == ShortcutState::Pressed`** —
   otherwise every keypress would fire twice, on press and release.
2. Spawns a task (the handler is sync; `AppState` uses async locks).
3. Reads `AppState.spotify`; returns if `None` — the keys are registered at
   startup, long before any login.
4. Calls the matching `Spirc` method.
5. Logs a warning on failure.

Registration failures are logged, not propagated.

## Inputs / outputs / side effects

- **Registers global OS hotkeys** — system-wide, affecting other applications.
- **Controls playback** via `Spirc`.
- Spawns a short-lived task per keypress.

## Dependencies

**Imports:** `tauri::{AppHandle, Manager}`, `tauri_plugin_global_shortcut`,
[[state.rs]]
**Imported by:** [[lib.rs]] (called from the `setup` hook)

## Notable logic / gotchas

> **Registration is best-effort by design.** Media keys are a globally
> exclusive resource: if another player already holds them, registration fails.
> The module logs a warning and continues rather than failing startup. The
> practical consequence is that **media keys can silently not work** — check the
> log for the registration warning before assuming a bug.

- **Registered before login.** The `setup` hook runs at startup, so handlers
  must tolerate a logged-out state. They do, via the early `None` return.
- **`ShortcutState::Pressed` check is essential.** Without it, one physical
  press produces two commands — pressing Next would skip two tracks.
- **No capability entry is required.** Capabilities gate the *frontend* IPC
  surface; Rust-side plugin use is ungated. See [[tauri-config]].
- Keys are never unregistered — they live for the process lifetime, released by
  the OS on exit.
- Commands go through `Spirc`, so a media key and an on-screen click take the
  identical path. See [[playback-and-connect]].

## See also

[[lib.rs]] · [[state.rs]] · [[playback-and-connect]] · [[tauri-config]] ·
[[known-limitations]] · [[entry-points]] · [[backend-rust]] · [[MOC]]
