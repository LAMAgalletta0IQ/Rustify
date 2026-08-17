---
tags: [file, backend, rust]
---
# `src-tauri/src/error.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **71 lines**

## Purpose

Defines the one error type that crosses the IPC boundary. Every backend
function returns `AppResult<T>`, so failures reach the frontend in a uniform,
machine-readable shape.

## Key items

### `enum AppError`

| Variant | Meaning |
| --- | --- |
| `NotLoggedIn` | No live session. Returned by the `token` / `with_spirc` guards |
| `PremiumRequired(String)` | Account plan is not `premium`; carries the actual plan |
| `Auth(String)` | OAuth flow or refresh failed |
| `Playback(String)` | librespot error |
| `WebApi(String)` | HTTP or Spotify API error |
| `Other(String)` | Everything else |

Derives `thiserror::Error`, so each variant carries a `Display` message. The
`PremiumRequired` message explicitly explains that librespot cannot play the
free tier — see [[auth-and-tokens]].

### `fn kind(&self) -> &'static str`
Returns a stable string tag per variant.

### `impl Serialize for AppError`
Hand-written rather than derived, producing:
```json
{ "kind": "PremiumRequired", "message": "A Spotify Premium subscription is required..." }
```

> **Why hand-written.** A derived `Serialize` on an enum produces a tagged
> union that is awkward to consume from TypeScript. This flat shape lets the
> frontend branch on `kind` without string-matching human-readable messages —
> exactly what [[Login.svelte]] does for `PremiumRequired`.

### `From` conversions
- `librespot::core::Error` → `Playback`
- `reqwest::Error` → `WebApi`
- `std::io::Error` → `Other`

These make `?` work throughout the backend.

### `type AppResult<T> = Result<T, AppError>`

## Inputs / outputs / side effects

Pure. No I/O.

## Dependencies

**Imports:** `serde`, `thiserror`, `librespot::core::Error`, `reqwest::Error`
**Imported by:** every backend module — [[commands.rs]], [[auth.rs]],
[[player.rs]], [[webapi.rs]], [[library.rs]], [[search.rs]], [[connect.rs]],
[[queue.rs]]

## Notable logic / gotchas

- **The `librespot::core::Error` conversion flattens detail.** Everything from
  librespot becomes `Playback`, including what are really network or auth
  failures. A source comment notes this: callers wanting a different variant
  must map explicitly rather than using `?`.
- `kind` values are part of the frontend contract. [[types.ts]] mirrors them in
  `AppErrorPayload["kind"]`; renaming one silently breaks the branch in
  [[Login.svelte]].
- Tauri requires command error types to implement `Serialize`; that requirement
  is the reason this type exists rather than `anyhow`.

## See also

[[state.rs]] · [[commands.rs]] · [[types.ts]] · [[Login.svelte]] ·
[[auth-and-tokens]] · [[backend-rust]] · [[MOC]]
