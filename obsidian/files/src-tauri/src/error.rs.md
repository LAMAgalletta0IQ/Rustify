---
tags: [file, backend, rust]
---
# `src-tauri/src/error.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~166 lines**

> Until 2026-08 this had 12 variants and one `From` conversion besides
> `librespot`/`reqwest`. Five variants and a `From<crate::jams::JamError>`
> conversion were added as later features (Jam, DJ, Pathfinder-backed
> features) needed finer-grained failure reporting than `Other` gave them.

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
| `SessionExpired` | HTTP 401; requires sign-in |
| `BadRequest(String)` | HTTP 400 with Spotify's message |
| `Forbidden(String)` | HTTP 403; scope/access-mode/endpoint refusal |
| `Unavailable(String)` | HTTP 404 item or endpoint unavailable |
| `ServiceUnavailable { status }` | Spotify/provider HTTP 5xx |
| `RateLimited { retry_after: Option<u64> }` | HTTP 429, carrying Spotify's `Retry-After` in seconds |
| `Other(String)` | Everything else |
| `FeatureUnsupported(String, String)` | A feature that only exists via Spotify's private, unofficial internal APIs (queue reorder/remove, Blend creation, ...) — never reachable through the public Web API or a supported librespot API, so it was never implemented rather than half-implemented against something unstable. `#[allow(dead_code)]` — reserved for a feature the app deliberately doesn't attempt yet |
| `PublicApiLimitation(String)` | The public Web API used to support this but Spotify has since removed/restricted it (e.g. `/artists/{id}/top-tracks`, Feb 2026) — distinct from `FeatureUnsupported`, which never had a public endpoint at all. Also `#[allow(dead_code)]` |
| `EndpointNotAvailable(String)` | A specific documented endpoint has no reachable equivalent for this request shape right now (private endpoint required, or a scope/product tier this account lacks) |
| `PersistedQueryExpired(String)` | Spotify rotated a Pathfinder persisted-query hash out from under the app — see [[spotify/pathfinder.rs]]'s multi-candidate-hash resilience pattern |
| `LexiconUnavailable(String)` | DJ's dynamic `your_dj` Lexicon context failed to resolve — see [[spotify/dj.rs]] |

Derives `thiserror::Error`, so each variant carries a `Display` message. The
`PremiumRequired` message explicitly explains that librespot cannot play the
free tier — see [[auth-and-tokens]].

### `fn kind(&self) -> &'static str`
Returns a stable string tag per variant.

`RateLimited`'s `Display` appends "— retry in about {n}s" when the header was
present. Its doc comment records the cause: librespot's default client ID is
shared by every librespot-based client, so the quota is consumed globally and a
429 can appear on a first request. See [[rate-limiting]].

### `impl Serialize for AppError`
Hand-written rather than derived, producing **three** fields:
```json
{ "kind": "RateLimited", "message": "Spotify is rate limiting this client — retry in about 58s.", "retryAfter": 58 }
```

`retryAfter` is `null` for every other variant. It is exposed as its own field
so [[Login.svelte]] can drive a countdown instead of parsing the number back out
of the message text.

> **Why hand-written.** A derived `Serialize` on an enum produces a tagged
> union that is awkward to consume from TypeScript. This flat shape lets the
> frontend branch on `kind` without string-matching human-readable messages —
> exactly what [[Login.svelte]] does for `PremiumRequired`.

### `From` conversions
- `librespot::core::Error` → `Playback`
- `reqwest::Error` → `WebApi`
- `std::io::Error` → `Other`
- `crate::jams::JamError` → `Unavailable`/`Forbidden`/`Auth`/`BadRequest`/`Other`,
  mapped per-variant (`JamNotFound` → `Unavailable`, `PermissionDenied` →
  `Forbidden`, `ClientTokenExpired` → `Auth`, `Config` → `BadRequest`,
  everything else → `Other`) so the Jams module's own error taxonomy still
  lets the UI branch on `kind` the same way it does for every other command.
  `friends.rs`'s `classify_spclient_error` (see [[friends.rs]]) follows the
  same pattern for librespot's structured `http_client` errors, mapping them
  onto these variants explicitly instead of flattening through `Playback`.

These make `?` work throughout the backend.

### `type AppResult<T> = Result<T, AppError>`

## Inputs / outputs / side effects

Pure. No I/O.

## Dependencies

**Imports:** `serde`, `thiserror`, `librespot::core::Error`, `reqwest::Error`,
`crate::jams::JamError`
**Imported by:** every backend module — [[commands.rs]], [[auth.rs]],
[[player.rs]], [[webapi.rs]], [[library.rs]], [[search.rs]], [[connect.rs]],
[[queue.rs]], [[friends.rs]], [[jams_bridge.rs]], and the rest of the modules
added since 2026-08 (see [[backend-rust]])

## Notable logic / gotchas

- **The `librespot::core::Error` conversion flattens detail.** Everything from
  librespot becomes `Playback`, including what are really network or auth
  failures. A source comment notes this: callers wanting a different variant
  must map explicitly rather than using `?`.
- `kind` values are part of the frontend contract. [[types.ts]] mirrors them in
  `AppErrorPayload["kind"]`; renaming one silently breaks the branch in
  [[Login.svelte]].
- **`RateLimited` is separate from `WebApi` on purpose.** It is recoverable by
  waiting, so the UI treats it differently from a real failure — see
  [[rate-limiting]].
- Tauri requires command error types to implement `Serialize`; that requirement
  is the reason this type exists rather than `anyhow`.

## See also

[[state.rs]] · [[commands.rs]] · [[types.ts]] · [[Login.svelte]] ·
[[auth-and-tokens]] · [[backend-rust]] · [[MOC]]
