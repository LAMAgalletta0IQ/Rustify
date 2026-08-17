---
tags: [file, frontend, ui, auth]
---
# `src/lib/views/Login.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **106 lines**

## Purpose

The logged-out screen. Starts the OAuth flow and — importantly — presents the
Premium-required case as an explanation rather than a generic failure.

## Key items

### State
`busy`, `errKind`, `errMsg` — all local. This view deliberately does **not**
use `store.error`.

### `async doLogin()`
Sets `busy`, calls `api.login()`, and assigns the result to `store.auth`. On
rejection it captures both `kind` and `message` from `asAppError(e)`.

While `busy`, the button reads "Waiting for browser…" and a hint explains that
the browser has opened and to return after approving.

### Error rendering
```svelte
<div class="err" class:premium={errKind === "PremiumRequired"}>
```
`PremiumRequired` gets an amber treatment, a "Premium required" heading, and an
extra paragraph stating that librespot cannot stream the free tier and that
this is a limitation of the playback library rather than something the app can
work around. Any other error gets a red "Login failed" block.

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
- **Assigns `store.auth` directly** rather than waiting for the `auth:changed`
  event. The backend emits it too, so this is belt-and-braces; the direct
  assignment guarantees an immediate transition.
- The view does not distinguish "user closed the browser" from other OAuth
  failures — both surface as `Auth`.

## See also

[[auth-and-tokens]] · [[error.rs]] · [[auth.rs]] · [[App.svelte]] ·
[[api.ts]] · [[frontend-views]] · [[MOC]]
