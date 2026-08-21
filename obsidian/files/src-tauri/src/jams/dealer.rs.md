---
tags: [file, backend, jam, dealer, rust]
---
# `src-tauri/src/jams/dealer.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **333 lines**

WebSocket listener for `wss://dealer.spotify.com`, decoding the real-time
push side of a jam (member joined/left, queue changed, permissions changed).
Note this is a **separate** dealer connection from librespot's own — see the
gotcha below, this cost real debugging time.

## Key items

### `enum JamEvent`
Typed variants are best-effort matches on the `type` string seen in
captured payloads — internal and undocumented, so anything unrecognised
surfaces as `JamEvent::Unknown` instead of being silently dropped. Serialised
to the webview as-is (`camelCase`) when forwarded by [[jams_bridge.rs]].

### `struct DealerClient`
Owns the socket, the reconnect loop, and the `session_update` /
`broadcast_status_update` topic subscriptions.

## Dependencies

**Imports:** `tokio-tungstenite`, `serde_json` — no crate-internal imports
**Imported by:** [[mod.rs|jams/mod.rs]], [[session.rs]]

## Notable logic / gotchas

> ### A second dealer connection needs its own connection id
> Realtime jam state reuses the *concept* of librespot's authenticated Dealer
> connection but is its own socket with its own server-assigned connection
> id — it cannot represent the Connect device named by `local_device_id`
> unless that id is threaded through explicitly. [[jams_bridge.rs]] copies
> librespot's `Session::connection_id()` in before every command for exactly
> this reason; inventing a random one produced an empty-bodied 400 with no
> useful message.

## See also

[[jams_bridge.rs]] · [[session.rs]] · `README_jams.md` · [[MOC]]
