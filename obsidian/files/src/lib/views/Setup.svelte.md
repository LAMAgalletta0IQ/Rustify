---
tags: [file, frontend, ui, auth]
---
# `src/lib/views/Setup.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5

## Purpose

The first-run gate: shown instead of [[Login.svelte]] whenever no Web API
Client ID is configured yet. Walks the user through registering a Spotify
Developer app and saves the Client ID before Login (and therefore any OAuth
flow) becomes reachable. See [[auth-and-tokens]] for why a Client ID cannot
have a built-in default.

## Key items

### `onDone: () => void` prop
Called after a successful save. [[App.svelte]] wires this to
`store.finishSetup()`, which re-checks `get_login_info` and clears
`store.setupNeeded`, swapping the view for Login.

### `info: LoginInfo | null`
Loaded in `onMount` from `api.getLoginInfo()`, same defensive pattern as
[[Login.svelte]] — `null` until it resolves, and the redirect URI shown falls
back to the hardcoded string in that window so the guide is never blank.

### `save()`
Trims the input, no-ops on empty, calls `api.setClientId(trimmed)`, and on
success invokes `onDone()`. Failures surface via `asAppError(e).message` in
the same inline error block style as [[Login.svelte]].

## Inputs / outputs / side effects

**Side effects:** writes `settings.json` in the app data dir via the
`set_client_id` command. No network calls of its own.

## Dependencies

**Imports:** [[api.ts]]
**Imported by:** [[App.svelte]]

## Notable logic / gotchas

- **This view exists so `webapi_client_id` never needs a built-in fallback.**
  Before it existed, an unconfigured install silently shared the app author's
  quota — see the history note in [[auth-and-tokens]].
- **Also reachable after first run**, via [[Login.svelte]]'s "Use a different
  Client ID" link, which sets `store.setupNeeded = true` directly rather than
  calling a command — there is nothing to undo on the backend until the user
  actually saves a new ID.
- Uses the same `.wrap`/`.card` layout shape as [[Login.svelte]] rather than a
  shared component — both are small enough that extracting one felt like
  premature abstraction.

## See also

[[auth-and-tokens]] · [[Login.svelte]] · [[App.svelte]] · [[api.ts]] ·
[[types.ts]] · [[frontend-views]] · [[MOC]]
