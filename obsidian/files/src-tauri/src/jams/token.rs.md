---
tags: [file, backend, jam, rust]
---
# `src-tauri/src/jams/token.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **46 lines**

The single seam where the Jams module meets the host app's session.
Everything else in `jams/` treats a token as an opaque string; the host
updates it on login/refresh and the module never mints one itself.

## Key items

### `trait TokenProvider`
Supplies the OAuth bearer token identifying the user.

### `struct AccessToken(Arc<Mutex<Option<String>>>)`
A boxed, update-in-place token the host can swap out whenever OAuth
refreshes. `Default` yields an unauthenticated (`None`) token, which lets
the module run before any login has happened.

Also defines `ClientIdentity` and `ConnectionId`, the other two host-supplied
values (device id/client id, and the dealer connection id) — see
[[jams_bridge.rs]] for where all three actually get filled in.

## Dependencies

**Imports:** `std::sync` — no crate-internal imports
**Imported by:** [[mod.rs|jams/mod.rs]], [[jams_bridge.rs]], [[session.rs]]

## See also

[[jams_bridge.rs]] (the concrete implementation, mirroring librespot's own
session credentials) · [[mod.rs|jams/mod.rs]] · [[MOC]]
