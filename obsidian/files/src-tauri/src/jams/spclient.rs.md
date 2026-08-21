---
tags: [file, backend, jam, spclient, rust]
---
# `src-tauri/src/jams/spclient.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **747 lines**

HTTP client for the SpClient side of a jam — the actual
`social-connect/v2/sessions/...` request/response calls (create, current,
join, leave, kick, end, add track, queue control). The largest file in the
module; this is where the endpoint routing history lives.

## Key items

### `struct JamApiClient`
Builds each request against `JamConfig`'s (overridable) endpoint paths, with
the real Connect device id, `client-token` and dealer connection id supplied
by [[jams_bridge.rs]] before every call — never invented locally. See
`README_jams.md`'s "two bearers, and the one that gets you a 403" for why
the bearer must be librespot's login5 token, not the app's Web API token.

### `fn join_jam(jam_id) -> Result<Value, JamError>`
Joins by **join token** (the trailing segment of an invite link), not the
session id and not a `spotify.link` shortlink id — [[session.rs]]'s
`resolve_join_token` sorts those out before calling this.

### `fn add_track(jam_id, track_uri)`
A jam's queue *is* the Connect queue, so this is the public Web API queue
endpoint by default; `jam_id` is unused, kept only so the call site reads
the same as the others.

### `fn reorder_queue(...)`
Not available — no captured path exists for reordering a jam queue, so this
errors rather than guessing one.

## Dependencies

**Imports:** `reqwest`, `serde_json` — no crate-internal imports
**Imported by:** [[session.rs]] (`JamManager`)

## Notable logic / gotchas

> ### The join path was on `v3`, which 404s
> Until 2026-08 the default join path used `social-connect/v3/...`, which
> 404s. Only the join path was ever in doubt — the rest of social-connect
> answers on `v2`, and so does join. A brief fallback chain that tried
> several shapes on 404 established that and was removed once confirmed;
> the endpoint config now ships `v2` as the default with no fallback needed.

## See also

`README_jams.md` (the full endpoint table and bearer-token rules) ·
[[session.rs]] · [[jams_bridge.rs]] · [[dealer.rs]] · [[MOC]]
