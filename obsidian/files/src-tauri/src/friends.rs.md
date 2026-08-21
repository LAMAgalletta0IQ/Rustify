---
tags: [file, backend, dealer, social, rust]
---
# `src-tauri/src/friends.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **664 lines**

Event-driven friend-presence feed. The public Web API has no friend-
presence surface; Spotify's own clients seed this from SpClient and receive
cheap per-user invalidation pushes over the already-authenticated Dealer
connection — a push carries no activity payload, only which single profile
to refetch.

## Key items

### `async fn seed(session, connection_id) -> AppResult<Vec<FriendActivity>>`
`GET /presence-view/v2/init-friend-feed/{connectionId}` — the initial feed.

### `async fn fetch_user(session, user_id) -> AppResult<Option<FriendActivity>>`
`GET /presence-view/v1/user/{id}` — per-user refresh on a Dealer push for
`hm://presence2/user/`.

### `fn classify_spclient_error(context, error) -> AppError`
Maps librespot's structured `Error { kind, .. }` (`ErrorKind::NotFound` →
403/404-equivalent, `Unauthenticated` → 401, `ResourceExhausted` → 429, etc.
— set by `librespot_core::http_client` from the real HTTP status) onto the
app's own `AppError` variants, so callers can tell a genuine capability
refusal from a transient failure. Added 2026-08.

### `fn is_capability_absent(error) -> bool`
`true` only for `AppError::Unavailable`/`Forbidden` (404/403) — the two
outcomes that actually mean "this account/region lacks the capability".

### `pub fn spawn(app, session) -> AppResult<JoinHandle<()>>`
The long-lived task: re-seeds every `RESEED_INTERVAL` (30 min) or on a
Dealer reconnect, applies per-user pushes as they arrive, emits
`FriendFeed` on every change.

## Dependencies

**Imports:** `librespot::core::{session::Session, SpotifyUri}`, `http`,
`futures_util`, [[state.rs]] (`events`, `AppState`)
**Imported by:** [[commands.rs]] (`spawn` at login), [[state.rs]] (`friends_task`)

## Notable logic / gotchas

> ### Every failure used to say "not available in your region"
> Until 2026-08 `seed`/`fetch_user` flattened librespot's structured error
> into one generic string, so `apply_seed`'s failure branch had no way to
> tell a 403/404 (genuine capability refusal) apart from an expired token, a
> rate limit, or a dropped Dealer connection — and since nothing is cached
> on first launch, *any* failure was reported as `FriendFeedStatus::Unavailable`,
> worded in the UI as "not available for this account or region". Fixed by
> `classify_spclient_error`/`is_capability_absent`: only an actual 403/404
> sets `Unavailable` now; everything else sets a distinct `Failed` status,
> worded as transient. Same fix applied to the Dealer-connection-lost path
> and the initial subscription-registration failure in [[commands.rs]].

## See also

[[state.rs]] · [[commands.rs]] · [[known-limitations]] · [[MOC]]
