---
tags: [file, backend, jam, rust]
---
# `src-tauri/src/jams/mod.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **122 lines**

Entry point and public surface of the Spotify Jam module. Deliberately
self-contained — depends only on standard crates and exposes
[`token::TokenProvider`] as the single seam where the host app's OAuth
session plugs in. Nothing in `jams/` imports from the rest of the crate; the
host ([[jams_bridge.rs]]) supplies credentials and forwards events, the
module supplies the protocol. Full protocol writeup, bearer-token gotchas
and current status: `README_jams.md` at the repo root.

## Key items

Re-exports the module's public API: `JamManager` (the façade
[[jams_bridge.rs]] drives), `JamSession`/`JamMember`/`JamTrack` (the typed
model — see [[session.rs]]), `JamError` ([[error.rs]]), `JamConfig`
([[config.rs]]), `JamCredentials`/`ClientIdentity`/`ConnectionId`
([[token.rs]]).

## Dependencies

**Imports:** `client_token`, `config`, `dealer`, `error`, `pathfinder`,
`session`, `spclient`, `token` (all siblings in this directory)
**Imported by:** [[jams_bridge.rs]]

## See also

[[jams_bridge.rs]] · [[session.rs]] · [[spclient.rs]] · [[known-limitations]] · [[MOC]]
