# Jams module (experimental)

Spotify **Jams** (social listening sessions) as a self-contained Rust module at
`src-tauri/src/jams/`. It talks to three internal services:

| Service    | Transport    | Purpose                                          |
| ---------- | ------------ | ------------------------------------------------ |
| SpClient   | HTTPS        | request/response actions: create, join, update, leave a jam |
| Dealer     | WebSocket    | real-time push: track added, queue changed, members joined/left |
| Pathfinder | GraphQL      | metadata, restricted to **persisted queries**    |

Nothing in the module imports from the rest of the crate. The seam is
`jams::JamCredentials` — the host app supplies the bearers, the Connect device
id, the `client-token` and the dealer connection id; the module supplies the
protocol.

## Two bearers, and the one that gets you a 403

Rustify already juggles two OAuth logins (see `CLAUDE.md`). Jams add a third
token and a rule about which goes where:

| Host | Bearer |
| --- | --- |
| `spclient.wg.spotify.com`, `api-partner.spotify.com`, dealer | librespot's **login5** token (`Session::login5().auth_token()`) |
| `api.spotify.com` | the app's **Web API** token, from `TokenStore` |

spclient is first-party. Its RBAC filter answers a token minted for a
self-registered Client ID with

```text
403 RBAC: access denied
```

**regardless of scopes** — there is no scope that buys a third-party app into
social-connect, so this is not something the Setup screen's Client ID can fix.
librespot's login5 token is issued under Spotify's own desktop client id (the
same one that mints the `client-token`), so bearer, client-token and device all
belong to one client, which is what these endpoints check.

> **login5, not keymaster.** The obvious first-party token source is
> `Session::token_provider().get_token(scopes)`, and it does not work:
> `hm://keymaster/token/authenticated` answers **403 "Invalid request"** for an
> OAuth-authenticated session whatever scopes you ask for. librespot's own
> `SpClient` stopped using it too — `spclient.rs` calls
> `login5().auth_token()`. login5 mints from the *stored credentials*, so it
> also needs the credentials cache to be populated (it is, after any
> successful login).

The credentials are pulled in `jams_bridge::refresh`, which runs once during
`JamController::build` — before the dealer task is spawned, or its first
connect fails on an empty cell — and again before every command.

The public Web API keeps using the app's own token: that is what it is for, and
it keeps that traffic on the user's own quota. `JamApiClient::bearer_for`
picks by host, which matters because `add_track` is an `api.spotify.com` call
sitting among spclient ones.

> Until 2026-08 only the bearer came from the host and the module invented the
> rest: a random device id, a self-minted `client-token`, and a self-chosen
> `Spotify-Connection-Id`. All three are wrong. Spotify binds a jam to a
> **Connect device**, pushes its updates over the **dealer connection that
> device is registered on**, and issues client tokens through a protobuf
> handshake with a hashcash challenge. An invented value in any of the three is
> answered with an **empty-bodied 400** that names nothing. librespot already
> holds all three (`Session::device_id()`, `Session::connection_id()`,
> `SpClient::client_token()`), so `jams_bridge` copies them in before every
> command rather than reimplementing them.

## Wired into the app

The **Jams** tab (`src/lib/views/Jams.svelte`) drives it from the UI: Create /
Join / Leave / Add current track, a live dealer event feed, and a status line
that reports how many endpoints/hashes are captured. The glue is
`src-tauri/src/jams_bridge.rs` (`JamController`), which builds the module
lazily on first use, mirrors the session's Web API bearer token into it, and
forwards dealer events to the webview as `jams:changed`. Commands:
`get_jam_status`, `create_jam`, `refresh_jam`, `join_jam`, `leave_jam`,
`add_track_to_jam`, `set_jam_queue_control`, `kick_jam_member`, and `end_jam`.

Realtime session state reuses librespot's authenticated Dealer connection and
subscribes to `social-connect/v2/session_update` plus
`social-connect/v2/broadcast_status_update`. This is important: a second Dealer
socket has a different server-assigned connection id and cannot represent the
Connect device named by `local_device_id`.

`jams.toml` is read from the **app data dir** (next to `settings.json`); absent,
the built-in defaults apply (social-connect endpoint paths, no hashes). Keys
present in the file override the defaults one by one — a `[spclient_endpoints]`
table containing only `join_jam` leaves the other four alone.

## Endpoints: defaulted, still overridable

Jams are the current name for what the backend calls a **social connect
session** (formerly Group Sessions), and that is the service the actions hit —
not a `/jam/v1/...` API. Those paths are stable enough to ship as defaults
(`config::default_spclient_endpoints`):

| Key           | Verb | Path                                            |
| ------------- | ---- | ----------------------------------------------- |
| `create_jam`  | GET  | `/social-connect/v2/sessions/current_or_new`    |
| `current_jam` | GET  | `/social-connect/v2/sessions/current`           |
| `join_jam`    | POST | `/social-connect/v2/sessions/join/{jam_id}`     |
| `leave`       | POST | `/social-connect/v2/sessions/leave`             |
| `add_track`   | POST | `https://api.spotify.com/v1/me/player/queue`    |
| `queue_control_allowed` | PUT | `/social-connect/v2/sessions/current/queue_only_mode/disabled` |
| `queue_control_denied` | PUT | `/social-connect/v2/sessions/current/queue_only_mode/enabled` |
| `kick_member` | POST | `/social-connect/v3/sessions/{jam_id}/member/{member_id}/kick` |
| `end_jam` | DELETE | `/social-connect/v3/sessions/{jam_id}` |

Notes:

- **`{jam_id}` on `join_jam` is the join *token*, not the session id** — the
  trailing segment of `https://open.spotify.com/socialsession/<token>`.
  `session::join_token()` accepts the full link, the
  `spotify:socialsession:<token>` URI, or the bare token. It takes the **last
  non-empty path segment** rather than matching on the segment name, because
  Spotify hands out several link shapes (`/jam/<token>`, `spotify.link`
  shortlinks, locale-prefixed paths) and a name-matching parser fell back to
  the whole URL on all of them — sending `https:` as the join token.
- **A `spotify.link` shortlink must be followed before joining.** The share
  sheet gives out `https://spotify.link/<id>`, whose last path segment is an
  11-character *shortlink id*, not a join token — sending it earns
  `400 BAD_JOIN_TOKEN`. `JamManager::resolve_join_token` follows the redirect
  (unauthenticated, reqwest does it automatically) and reads the token off the
  `socialsession` URL it lands on. A link that already says `socialsession` is
  parsed directly, with no network call.
- **A jam action with no JSON body still needs `Content-Length: 0`.** The
  Google frontend in front of spclient answers a bodyless POST with
  `411 Length Required` and an HTML page, before Spotify sees the request, so
  `post_with_query` sends an empty byte body rather than no body at all.
- **Every action carries `local_device_id`**, which must be this app's
  librespot Connect device id (`jams_bridge` passes
  `Session::device_id()`). A jam is bound to a device; a random id would open
  a session that controls no player. Without a live session the call fails
  with a config error rather than an opaque 400.
- **Every action also carries `Spotify-Connection-Id`** — librespot's, so it
  belongs to the same client as the device id above. It is empty until Spirc
  has received it, and the header is then omitted rather than faked; the sync
  logs a warning when that happens.
- **`add_track` is the public Web API queue endpoint**, because a jam's queue
  *is* the Connect queue. It is absolute, so it bypasses the spclient host.
- **`reorder_queue` has no default and still refuses.** Nothing public
  documents a social-connect reorder call, and guessing would fire a real
  request. Capture it and add the key to use it.
- **Verbs and query parameters are fixed in code**, only paths are
  configurable — they differ per endpoint, so one uniform "POST JSON" shape
  cannot serve them all.

> Until 2026-08 `spclient_endpoints` shipped **empty**, and every action
> answered `no endpoint path configured for '…'; capture it with a traffic
> proxy`. The reasoning was sound — a guessed path fails as an opaque 400 and
> a loud config error says exactly what to capture — but it made the feature
> unusable without a mitmproxy session first, for paths that turned out to be
> the long-lived social-connect ones. The escape hatch is unchanged: these are
> defaults in config, so drift is fixed by editing `jams.toml`, not by
> rebuilding.

Still genuinely capture-only:

1. **Pathfinder persisted-query hashes** (`JamConfig.pathfinder_hashes`). You
   cannot send arbitrary GraphQL. You must replay the exact `operationName` +
   `sha256Hash` pair from the official client. Unregistered operations fail
   with `JamError::PersistedQueryExpired` telling you what to capture.
   `hydrate()` treats this as optional enrichment, so create/join work without
   it.

2. The Dealer **connect handshake** shape and the Jam event `type` strings
   (`dealer.rs`). Both are best-effort and can drift; a feed showing only
   `Unknown` events means the `type` strings need updating.

`JamApiClient` keeps raw `post`/`get`/`post_with_query` for probing a captured
path before committing it to config.

The three clients also share one `Spotify-Connection-Id` (see **Reliability
design**); while capturing, check that the official client's jam requests carry
a `Spotify-Connection-Id` header matching the value in its dealer `connect`
handshake, so the plumbing below is confirmed rather than assumed.

## Capturing with mitmproxy

1. Install mitmproxy and trust its CA on the machine running the official
   Spotify desktop/web client.
2. Filter for the hosts above:
   `mitmdump --set flow_detail=2 -q --ignore-hosts 'spotify.com'` tuned to the
   right hosts (spclient.wg.spotify.com, api-partner.spotify.com,
   clienttoken.spotify.com, dealer.spotify.com).
3. Create/join a jam and add a track; record the request paths and bodies, the
   Pathfinder operation names + hashes (they travel in the request body under
   `extensions.persistedQuery.sha256Hash`), and the dealer `type` strings.

## Updating when things expire

**Pathfinder hash expired** — a query returns `PersistedQueryExpired` with the
operation and hash. Re-capture the official client's request for that operation
and either:

- edit `pathfinder_hashes` in your `jams.toml`, or
- call `PathfinderClient::register_hash(op, hash)` at runtime before the query.

**Endpoint path changed / 404** — `spclient_endpoints` entries use `{jam_id}`
placeholders that are substituted at call time. Update the template in the
config; it wins over the default. Canonical key names used by the action
methods:

```
create_jam, current_jam, join_jam, add_track, reorder_queue, leave
```

**client-token refused** — in the app there is nothing to fix here: the token
comes from librespot (`SpClient::client_token()`), which speaks the protobuf
protocol and solves the hashcash challenge the service now issues.
`ClientTokenManager` still has a JSON mint for standalone use
(`examples/jam_demo.rs`), but expect it to be refused: the service answers a
plain JSON request with `RESPONSE_CHALLENGES_PRESENT`, and the `client_data`
shape has to match the client id (desktop ids want `connectivity_sdk_data`,
not `js_sdk_data`). A mint failure reports the mint URL, never "spclient
returned …", so it cannot be mistaken for an endpoint problem.

**Which request failed** — every non-2xx logs `spclient <METHOD> <url> ->
<status>: <body>` and the error repeats the URL, because these endpoints
answer both a stale path and a bad header with an empty-bodied 400 and the URL
is the only thing that separates them. `tracing` is built with its `log`
feature so those lines reach the app's `env_logger`; see them with
`$env:RUST_LOG="rustify_lib::jams=debug"`.

## Reliability design

- **client-token**: cached with a 60 s safety margin; refreshed lazily;
  minted only on first use. Never persisted.
- **Dealer**: application-level ping on a fixed interval, dead-connection
  detection via last-inbound-frame timestamp, and reconnect with exponential
  backoff (1 s → 30 s, reset after a session that survives 60 s). The event
  channel survives reconnects — consumers never re-subscribe.
- **Connection id**: a shared `jams::ConnectionId` keeps the dealer handshake
  and the HTTPS clients in agreement. The dealer rotates it to a fresh random
  id immediately before every (re)connect; SpClient and Pathfinder attach the
  current value as the `Spotify-Connection-Id` header on each request, which
  is how the server routes real-time pushes to this websocket.
- **Degradation**: `hydrate()` enriches a jam with Pathfinder metadata but
  treats Pathfinder failure as a warning, never an error — the SpClient
  response already carries the jam id.

## Verification

No test suite in this repo; the loop is:

```text
cd src-tauri
cargo check --no-default-features          # lib compiles
cargo check --examples --no-default-features
cargo run --example jam_demo               # wiring demo
```

Run the demo with `RUST_LOG="rustify_lib::jams=debug"` for module tracing.

## Example `jams.toml`

```toml
spclient_base_url  = "https://spclient.wg.spotify.com"
pathfinder_url     = "https://api-partner.spotify.com/pathfinder/v2/query"
client_token_url   = "https://clienttoken.spotify.com/v1/clienttoken"
dealer_url         = "wss://dealer.spotify.com"

# Only needed to override a default that has drifted.
[spclient_endpoints]
create_jam      = "/social-connect/v2/sessions/current_or_new"
current_jam     = "/social-connect/v2/sessions/current"
join_jam        = "/social-connect/v2/sessions/join/{jam_id}"
leave           = "/social-connect/v2/sessions/leave"
add_track       = "https://api.spotify.com/v1/me/player/queue"
# No default — capture it first.
reorder_queue   = "/social-connect/v?/sessions/{jam_id}/queue"

[pathfinder_hashes]
FetchJam = "1e3b6cd0b9a4..."
```
