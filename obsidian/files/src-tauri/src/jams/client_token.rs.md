---
tags: [file, backend, jam, rust]
---
# `src-tauri/src/jams/client_token.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **204 lines**

Manages Spotify's internal `client-token` header, required by Pathfinder and
most SpClient endpoints alongside the OAuth bearer. Minted from a generated
device id by the internal clienttoken service
(`https://clienttoken.spotify.com/v1/clienttoken`); short-lived, so this
caches the `ClientToken` and its expiry and re-mints only when needed rather
than on every request.

## Key items

### `struct ClientToken { value, expires_at }`
The minted token and its lifetime.

### enum for token source
Distinguishes a freshly-minted token from one supplied externally (e.g.
mirrored from librespot's own `SpClient::client_token()` — see
[[jams_bridge.rs]], which prefers that path over minting its own, since a
self-minted token under the wrong client id gets refused independently of
scope).

### `struct ClientTokenManager`
Owns the cache and the minting HTTP call.

## Dependencies

**Imports:** `reqwest`, `serde` — no crate-internal imports (self-contained
module rule)
**Imported by:** [[mod.rs|jams/mod.rs]], [[session.rs]]

## See also

[[jams_bridge.rs]] (why the host mirrors librespot's client-token rather than
letting this module mint its own) · [[mod.rs|jams/mod.rs]] · [[MOC]]
