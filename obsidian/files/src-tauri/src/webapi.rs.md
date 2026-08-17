---
tags: [file, backend, webapi, rust]
---
# `src-tauri/src/webapi.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **88 lines**

## Purpose

A minimal HTTP client for the Spotify Web API. Centralises the base URL, the
bearer header, error unwrapping, and the `204 No Content` quirk so the four
Web API modules stay free of HTTP concerns.

## Key items

### `const BASE: &str = "https://api.spotify.com/v1"`

### `struct WebApi { http: reqwest::Client }`
**Holds no token.** The caller passes it per request, so there is exactly one
credential in the app, owned by `TokenStore` in [[state.rs]]. Constructed with
a `User-Agent` of `spotify-rust/<version>` from `CARGO_PKG_VERSION`.

### `async fn send<T>(req, token) -> AppResult<T>`
The core path used by every verb:
1. Attach `bearer_auth(token)` and send.
2. **If `429`, read the `Retry-After` header** and return
   `AppError::RateLimited { retry_after }`, logging a warning. Handled before
   anything else because it is recoverable by waiting — see [[rate-limiting]].
3. **If `204 No Content`, deserialise from the literal `"null"`** — player
   PUT/POST endpoints answer with an empty body, which would otherwise fail
   JSON parsing.
4. On non-success, extract `error.message` from Spotify's
   `{"error":{"status","message"}}` envelope, falling back to the raw body, or
   to `"no response body"` when the body is empty.
5. Otherwise deserialise into `T`.

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
- `put`, `post`, `delete` — **no retry**. Repeating a `POST /me/player/queue`
  would double-queue a track.

## Inputs / outputs / side effects

**Network I/O only.** Every outbound HTTP request in the app originates here
(librespot's own traffic excepted).

## Dependencies

**Imports:** `reqwest`, `serde::de::DeserializeOwned`, `serde_json`,
[[error.rs]]
**Imported by:** [[commands.rs]], [[player.rs]], [[auth.rs]], [[connect.rs]],
[[library.rs]], [[search.rs]], [[queue.rs]]

## Notable logic / gotchas

- **The `204` special case is load-bearing.** Without it, `transfer_playback`
  and `add_to_queue` would report a parse error on success. `serde_json` maps
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
- Still no handling for `503` or generic transient failures — only 429.

## See also

[[rate-limiting]] · [[external-dependencies]] · [[commands.rs]] ·
[[library.rs]] · [[search.rs]] · [[connect.rs]] · [[queue.rs]] · [[error.rs]] ·
[[backend-rust]] · [[MOC]]
