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
2. **If `204 No Content`, deserialise from the literal `"null"`** — player
   PUT/POST endpoints answer with an empty body, which would otherwise fail
   JSON parsing.
3. On non-success, try to extract `error.message` from Spotify's
   `{"error":{"status","message"}}` envelope, falling back to the raw body.
4. Otherwise deserialise into `T`.

### Verb wrappers
- `get<T>(token, path, query)` — query params appended only if non-empty.
- `put(token, path, body)`, `post(token, path, body)`, `delete(token, path, body)`
  — each `send::<Value>` and discard the result.

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
- No retry or rate-limit handling. Spotify's `429` would surface as a plain
  `WebApi` error.

## See also

[[external-dependencies]] · [[commands.rs]] · [[library.rs]] · [[search.rs]] ·
[[connect.rs]] · [[queue.rs]] · [[error.rs]] · [[backend-rust]] · [[MOC]]
