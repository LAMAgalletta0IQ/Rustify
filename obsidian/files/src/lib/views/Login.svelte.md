---
tags: [file, frontend, ui, auth]
---
# `src/lib/views/Login.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **211 lines**

## Purpose

The logged-out screen. Starts the OAuth flow and — importantly — presents the
Premium-required and rate-limited cases as explanations rather than generic
failures.

## Key items

### State
`busy`, `errKind`, `errMsg`, `cooldown`, plus a `timer` handle — all local.
This view deliberately does **not** use `store.error`.

### `info: LoginInfo | null`
Loaded in `onMount` from `api.getLoginInfo()`, inside a `try`/`catch` so a
failure here can never block logging in. `null` until it resolves, and every
use is written to degrade gracefully in that window.

It drives two pieces of copy that would otherwise be wrong half the time:

| `privateClientId` | Browser hint | Rate-limit hint |
| --- | --- | --- |
| `true` | Warns that **two** tabs open, one for playback and one for library/search | Notes the quota is private, so a 429 is likely a short burst |
| `false` | Single tab | Explains the shared-quota cause **and** prints the fix: dashboard URL, `webapiRedirectUri`, `clientIdEnv` |

Those values come from the backend rather than being hardcoded, so they cannot
drift from [[auth.rs]].

### `async doLogin(silent = false)`
Sets `busy` and calls the backend. When `silent`, it first tries
`api.restoreSession()` and returns early if that succeeds; otherwise it falls
through to the interactive `api.login()`. On rejection it captures `kind` and
`message` from `asAppError(e)`, and starts a cooldown when the error is
`RateLimited` with a `retryAfter`.

While `busy`, the button reads "Waiting for browser…" and a hint explains that
the browser has opened and to return after approving — mentioning the **second**
tab when the split is active, since users otherwise assume the app has hung.

### `startCooldown(secs)` / `stopCooldown()`
A 1 Hz interval counting down `cooldown`; on reaching zero it calls
`doLogin(true)`. The button becomes `Retrying in {n}s…` and is disabled
throughout. `onDestroy(stopCooldown)` clears the timer.

Started with `retryAfter + 2` — retrying on the exact boundary tends to be
refused again.

### Error rendering
Three presentations, keyed off `errKind`:

| `kind` | Heading | Style |
| --- | --- | --- |
| `PremiumRequired` | "Premium required" | Amber |
| `RateLimited` | "Rate limited by Spotify" | Amber |
| anything else | "Login failed" | Red |

`PremiumRequired` adds a paragraph explaining that librespot cannot stream the
free tier and that this is a playback-library limitation, not something the app
can work around.

`RateLimited` adds a paragraph explaining that the shared librespot client ID's
quota is consumed globally, so a fresh login can be refused through no fault of
the user — and says whether it is waiting out the window automatically.

## Inputs / outputs / side effects

**Side effects:** triggers the `login` command, which **opens the system
browser** and blocks on a loopback redirect. Writes `store.auth` on success,
which causes [[App.svelte]] to swap in the main UI.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]]
**Imported by:** [[App.svelte]]

## Notable logic / gotchas

- **This is why `AppError` serialises with a `kind` field.** Branching on a
  stable tag rather than string-matching a human-readable message is the whole
  reason for the custom `Serialize` impl in [[error.rs]].
- **`busy` gates the button**, preventing a second OAuth attempt while one
  loopback listener is already bound to port 8898 — a second would fail to bind.

> ### The auto-retry must be silent
> The countdown calls `doLogin(true)`, which goes through `restoreSession()`.
> Calling `login()` instead would **reopen the browser every 60 seconds** —
> worse than the failure it is recovering from. This works only because
> [[commands.rs]] persists the refresh token *before* the Premium gate. See
> [[rate-limiting]].

> ### `onclick={() => doLogin()}`, never `onclick={doLogin}`
> The bare reference passes the `MouseEvent` as the `silent` argument, making
> every manual click take the silent path. Caught by `svelte-check`, which is
> why the wrapper arrow is required.
- **Assigns `store.auth` directly** rather than waiting for the `auth:changed`
  event. The backend emits it too, so this is belt-and-braces; the direct
  assignment guarantees an immediate transition.
- The view does not distinguish "user closed the browser" from other OAuth
  failures — both surface as `Auth`.

## See also

[[auth-and-tokens]] · [[rate-limiting]] · [[error.rs]] · [[auth.rs]] ·
[[commands.rs]] · [[App.svelte]] · [[api.ts]] · [[types.ts]] ·
[[frontend-views]] · [[MOC]]
