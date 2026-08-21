---
tags: [file, backend, jam, rust]
---
# `src-tauri/src/jams_bridge.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **547 lines**

Glue between the self-contained [[jams/mod.rs|jams/]] module and the app's
real librespot session. Owns one `JamManager` (and therefore one dealer
listener) for the controller's life, feeds it the Web API bearer from
[[state.rs]]'s `TokenStore`, and forwards dealer events to the webview as
`jams:changed`. Built lazily on the first jam command, dropped on logout
(which aborts the dealer task).

## Key items

### `async fn refresh(spotify, tokens, ...)`
Pulls every credential a jam call needs out of the live librespot session —
run once during `JamController::build` (before the dealer task spawns) and
again before every command, since these rotate independently and a stale
one fails with no useful message:
- **First-party bearer** from `spotify.login5().auth_token()` — **not** the
  Web API bearer. spclient's RBAC filter refuses a self-registered Client
  ID's token with `403 RBAC: access denied` regardless of scope; there is no
  scope that buys a third-party app into social-connect.
- **`client-token`** from `spotify.spclient().client_token()`.
- **Dealer connection id** from `spotify.connection_id()` — empty until
  Spirc has seen it, in which case the header is omitted rather than faked.

### `struct JamController`
Wraps `JamManager` plus the live `Session`, exposing `create`/`join`/`leave`/
`kick`/`end`/`set_queue_control`/`refresh_session`/`sync_token` to
[[commands.rs]].

## Dependencies

**Imports:** `librespot::core::session::Session`, [[jams/mod.rs|jams::*]],
[[state.rs]] (`TokenStore`, `events`)
**Imported by:** [[commands.rs]], [[state.rs]] (`AppState.jams` field)

## Notable logic / gotchas

> ### Three invented values, three 400s
> Until 2026-08 only the bearer came from the host session; the module
> invented its own device id, self-minted `client-token`, and self-chosen
> `Spotify-Connection-Id`. All three are wrong: Spotify binds a jam to a
> **Connect device**, pushes updates over the **dealer connection that
> device is registered on**, and issues client tokens through a protobuf
> handshake with a hashcash challenge. Any invented value earns an
> empty-bodied 400 naming nothing. librespot already holds all three, so
> `refresh()` copies them in rather than reimplementing them.

## See also

`README_jams.md` · [[jams/mod.rs|jams/]] · [[commands.rs]] · [[state.rs]] · [[MOC]]
