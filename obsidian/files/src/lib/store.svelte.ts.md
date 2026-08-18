---
tags: [file, frontend, state]
---
# `src/lib/store.svelte.ts`

**Module:** [[frontend-svelte]] · **Language:** TypeScript (Svelte 5 runes)

## Purpose

The frontend's single source of truth for playback and auth. Subscribes to
backend events, mirrors them into runes state, runs the local position ticker,
and provides shared error handling.

> The `.svelte.ts` extension is required — it tells the Svelte compiler to
> process runes (`$state`) in a plain TypeScript file. Renaming it to `.ts`
> breaks reactivity silently.

## Key items

### `class AppStore`

| Member | Type | Purpose |
| --- | --- | --- |
| `playback` | `$state<PlaybackState>` | Mirror of the backend snapshot |
| `auth` | `$state<AuthState>` | Profile / login status |
| `error` | `$state<string \| null>` | Banner text |
| `booting` | `$state(boolean)` | True until the restore attempt settles |
| `setupNeeded` | `$state(boolean)` | True until a Web API Client ID is configured; gates [[Login.svelte]] behind [[Setup.svelte]] |
| `#unlisten` | `UnlistenFn[]` | Event subscriptions |
| `#ticker` | `number \| null` | Interval handle |

### `async init()`
1. `listen(EVENT_PLAYBACK)` → replace `playback`, re-sync ticker.
2. `listen(EVENT_AUTH)` → replace `auth`.
3. `await api.getLoginInfo()`. If it reports no Client ID configured, set
   `setupNeeded = true` and **return** — nothing to restore yet.
4. Otherwise `await api.restoreSession()`.
5. If logged in, prime `playback` with `getPlayback()`.
6. **`finally { this.booting = false }`** — runs even on the early return.

### `async finishSetup()`
Re-fetches `getLoginInfo()` and clears `setupNeeded` once it reports a
configured Client ID. Called by [[App.svelte]] after [[Setup.svelte]]'s
`onDone` fires.

### `#syncTicker()`
Starts a 1 Hz `setInterval` while playing; clears it when not. Each tick adds
1000 ms, clamped to `durationMs`.

### `destroy()`
Calls every unlisten function and clears the ticker.

### `async run(fn)`
Clears `error`, awaits `fn()`, and on rejection sets
`error = asAppError(e).message`. The standard way components fire commands.

### `export const store = new AppStore()`
A module-level singleton — the same instance everywhere it is imported.

## Inputs / outputs / side effects

- **Subscribes** to two Tauri events.
- **Calls** `restore_session` and `get_playback` at startup.
- **Runs a timer** while playing.

## Dependencies

**Imports:** `@tauri-apps/api/event` (`listen`), [[api.ts]], [[types.ts]]
**Imported by:** [[App.svelte]] and every view and component

## Notable logic / gotchas

- **The `finally` block is load-bearing.** If `restoreSession()` rejects and
  `booting` never cleared, the app would hang on "Starting…" forever. A failed
  restore is logged as a warning and falls through to the login screen. The
  early return for `setupNeeded` relies on the same `finally` to clear
  `booting`, since `return` inside `try` still runs it.
- **1 Hz, not `requestAnimationFrame`.** A deliberate CPU trade-off for low-end
  hardware; the cost is up to a second of visual drift between backend
  corrections. See [[known-limitations]].
- **The ticker re-syncs on every playback event**, so pausing stops it promptly
  rather than waiting a tick.
- **Wholesale replacement makes the backend's position authoritative.** Every
  event discards the ticker's accumulated progress and adopts the payload's
  `positionMs`. That was correct in principle but broken in practice: events
  carrying no position (volume, shuffle, repeat) shipped a stale 0, so nudging
  the volume snapped the clock to 0:00 while audio played on. Fixed on the
  backend — [[state.rs]] now recomputes position before every emit — precisely
  so this side can stay a dumb overwrite.
- **Events replace state wholesale** (`this.playback = e.payload`) rather than
  merging. Correct, because the backend always sends a complete snapshot — see
  [[state-and-events]].
- **`playback` is primed only when logged in**, avoiding a `NotLoggedIn` error
  on the login screen.
- Private `#` fields are genuine JS private fields, so nothing outside the class
  can clear the ticker or the listeners.

## See also

[[state-and-events]] · [[api.ts]] · [[types.ts]] · [[App.svelte]] ·
[[Setup.svelte]] · [[state.rs]] · [[data-flow]] · [[frontend-svelte]] · [[MOC]]
