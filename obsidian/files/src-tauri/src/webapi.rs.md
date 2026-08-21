---
tags: [file, backend, webapi, rust]
---
# `src-tauri/src/webapi.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~198 lines**

## Purpose

A minimal HTTP client for the Spotify Web API. Centralises the base URL, the
bearer header, error unwrapping, and empty-success handling so the feature
Web API modules stay free of HTTP concerns.

## Key items

### `const BASE: &str = "https://api.spotify.com/v1"`

### `struct WebApi { http: reqwest::Client }`
**Holds no token.** The caller passes it per request, so there is exactly one
credential in the app, owned by `TokenStore` in [[state.rs]]. Constructed with
a `User-Agent` of `rustify/<version>` from `CARGO_PKG_VERSION`.

### `async fn send<T>(req, token) -> AppResult<T>`
The core path used by every verb:
1. Attach `bearer_auth(token)` and send.
2. **If `429`, read the `Retry-After` header** and return
   `AppError::RateLimited { retry_after }`, logging a warning. Handled before
   anything else because it is recoverable by waiting — see [[rate-limiting]].
3. Read the body, then map HTTP status to typed `BadRequest`,
   `SessionExpired`, `Forbidden`, `Unavailable`, `RateLimited`, or
   `ServiceUnavailable` as appropriate.
4. On non-success, extract `error.message` from Spotify's
   `{"error":{"status","message"}}` envelope, falling back to the raw body, or
   to `"no response body"` when the body is empty.
5. On any successful empty body (200 or 204), deserialize the literal `null`;
   otherwise deserialize the body into `T`.

### Retry constants
```rust
const MAX_AUTO_RETRY_SECS: u64 = 8;
const MAX_RETRIES: u32 = 2;
```

### Verb wrappers
- **`get<T>(token, path, query)`** — retries on 429. GETs are safe to repeat, so
  a short window is absorbed here rather than surfaced: it waits the server's
  `Retry-After` (or `1 << attempt` when absent), up to `MAX_AUTO_RETRY_SECS`
  and `MAX_RETRIES`. Longer waits return `RateLimited` to the caller. The
  request is rebuilt each attempt, since a `RequestBuilder` is consumed on send.
- `put`, `post`, `put_query`, `post_query`, `delete_query` — **no retry**.
  Repeating a `POST /me/player/queue` would double-queue a track.
- Query-only library mutations send an explicit `Content-Length: 0`; Spotify's
  edge returned 411 for a bodyless PUT during live verification.

## Inputs / outputs / side effects

**Network I/O only.** Spotify Web API requests originate here. librespot and
[[mod.rs|lyrics/mod.rs]] own their separate traffic.

## Dependencies

**Imports:** `reqwest`, `serde::de::DeserializeOwned`, `serde_json`,
[[error.rs]]
**Imported by:** [[commands.rs]], [[player.rs]], [[auth.rs]], [[connect.rs]],
[[library.rs]], [[search.rs]], [[queue.rs]]

## Notable logic / gotchas

- **Every failed request is logged with method, URL, status and Spotify's own
  message.** `send` builds the `Request` rather than sending the builder
  directly, purely so the URL is available for that line. A bare
  `400 Bad Request` in the UI identifies nothing; `GET …/search?…&limit=20 ->
  400: Invalid limit` identifies the bug immediately — which is exactly how the
  search ceiling in [[search.rs]] was found.
- **An empty bearer token is called out explicitly.** Spotify answers a
  malformed `Authorization` header with **400**, not 401, so it is otherwise
  indistinguishable from a bad query.
- **Empty-success handling is load-bearing.** Without it, transfer, queue, and
  generic library writes can report a parse error on success. `serde_json` maps
  `null` onto `()` and `Value::Null`, which is why the trick works.
- **Error messages are unwrapped from Spotify's envelope**, so the frontend
  banner shows "Player command failed: Restriction violated" rather than raw
  JSON.
- **A new client per command.** [[commands.rs]] calls `WebApi::new()` each
  time, so reqwest's connection pooling is not exercised. Harmless at this
  request volume; see that note for the possible optimisation.
- **`reqwest` uses `native-tls`**, matching librespot's feature selection in
  [[Cargo.toml]] so only one TLS stack is compiled in.
- **An empty error body used to render as a bare `"429 Too Many Requests: "`.**
  That is what a real rate-limit incident looked like before the `RateLimited`
  variant existed — the header carrying the wait time was being discarded. The
  empty-body fallback now says `"no response body"`.
- **Only GETs retry**, and only through short windows. A minute-long
  `Retry-After` is returned to the UI so the user is not left watching a frozen
  view; [[Login.svelte]] runs a visible countdown instead.
- 502/503/504 are `ServiceUnavailable`; writes are never silently retried.

## See also

[[rate-limiting]] · [[external-dependencies]] · [[commands.rs]] ·
[[library.rs]] · [[search.rs]] · [[connect.rs]] · [[queue.rs]] · [[error.rs]] ·
[[backend-rust]] · [[MOC]]
