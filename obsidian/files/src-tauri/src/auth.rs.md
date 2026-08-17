---
tags: [file, backend, auth, rust]
---
# `src-tauri/src/auth.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **412 lines**

## Purpose

Everything about identity: the two OAuth flows, the scope lists, refresh-token
persistence, the background token refresher, and the Premium check. Conceptual
overview in [[auth-and-tokens]].

## Key items

### Constants
- `STREAMING_PORT = 8898` — preferred loopback port for the streaming login.
- `WEBAPI_PORT = 8899` — fixed loopback port for the Web API login.
- `STREAMING_SCOPES: &[&str]` — 15 scopes, the **full union**. Broad on purpose:
  this token is the fallback when a private Web API token is unavailable.
- `WEBAPI_SCOPES: &[&str]` — the same list minus `streaming`.
- `CLIENT_ID_ENV = "RUSTIFY_CLIENT_ID"` — read from `.env` via [[lib.rs]].
- `REFRESH_MARGIN = 5 min` — how early to renew.
- `MIN_REFRESH_DELAY = 30 s` — floor, so an already-expired token cannot spin.

### `fn streaming_client_id() -> String`
Always `SessionConfig::default().client_id`, Spotify's own desktop ID. **Not**
configurable: it is the only one reliably granted the `streaming` scope.

### `fn webapi_client_id() -> Option<String>`
The user's own client ID, when set and non-blank. `None` means Web API traffic
shares the streaming login and its globally pooled quota.

### Redirect helpers
- `streaming_redirect_uri()` — prefers `STREAMING_PORT`, falls back to an
  ephemeral port when it is busy. Safe because the desktop ID accepts any
  loopback port.
- `webapi_redirect_uri()` — fixed at `WEBAPI_PORT`. A self-registered app must
  declare this exact string in the Spotify dashboard, so it cannot vary.

### `struct StoredTokens`
```rust
refresh_token: String,                      // streaming
webapi_refresh_token: Option<String>,       // #[serde(default)]
```
What is persisted. **Access tokens are never written to disk.** The `default`
keeps files written before the split loadable.

### `struct SessionTokens`
The pair of logins backing one session: `streaming_access`, `streaming_refresh`,
`webapi: OAuthToken`, and `webapi_client_id` so the refresher reuses the issuing
ID. `shared()` points both roles at one token; `stored()` projects it back to
`StoredTokens`; `is_split()` compares the client IDs.

### `fn non_empty(&str) -> Option<String>`
Guards a blank token from ever reaching the token endpoint as if it were a
credential. Applied both when storing and when reading.

### Token file helpers
`load_stored_tokens`, `save_stored_tokens`, `clear_stored_tokens` — read/write
`tokens.json` in the Tauri app data dir. Loading returns `Option`, so a missing
or corrupt file is simply "not logged in" rather than an error.

### `async fn interactive_login() -> SessionTokens`
Opens the browser for the streaming authorization, then — when a private client
ID is set — **again** for the Web API one. Without one, returns
`SessionTokens::shared()` and logs a warning about the shared quota.

### `async fn restore_login(&StoredTokens) -> SessionTokens`
Silent renewal of both tokens. If the Web API refresh fails it degrades to the
shared token rather than erroring; see the gotcha below.

### `fn is_grant_rejected(&AppError) -> bool`
True only for `invalid_grant` / `invalid_client`. The sole justification for
deleting stored credentials — everything else is transient.

### `fn spawn_refresher(...) -> JoinHandle<()>`
The background loop, operating on whichever token serves the Web API role:
1. Sleep until `expires_at − REFRESH_MARGIN`, floored at `MIN_REFRESH_DELAY`.
2. Refresh under the issuing client ID.
3. On success: `tokens.set(new_access_token)`, update `expires_at`, and persist
   the refresh token **if it rotated**, into the correct field.
4. On failure: log a warning and retry after the margin + floor.

### `async fn fetch_profile_require_premium(api, token) -> AuthState`
`GET /v1/me`, then rejects any `product` other than `"premium"` with
`AppError::PremiumRequired`. Returns a populated `AuthState` on success, picking
the first profile image as the avatar.

## Inputs / outputs / side effects

- **Network:** Spotify OAuth endpoints (two client IDs); `GET /v1/me`.
- **Filesystem:** reads/writes/deletes `tokens.json`.
- **Environment:** reads `RUSTIFY_CLIENT_ID`.
- **Binds** loopback ports 8898 / 8899 during login.
- **Spawns** a long-lived tokio task (the refresher).
- **Opens the system browser** — twice when the split is active.

## Dependencies

**Imports:** `librespot_oauth`, `librespot::core::config::SessionConfig`,
`serde`, `std::net::TcpListener`, `tokio::time`, [[state.rs]] (`AuthState`,
`TokenStore`), [[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]], [[lib.rs]]

## Notable logic / gotchas

- **A failed Web API refresh must not propagate.** By that point the streaming
  refresh has already succeeded, and Spotify rotates refresh tokens on use — so
  the stored streaming token is dead and its replacement lives only in memory.
  Returning `Err` would drop the rotation and lock the user out on the next
  launch. It degrades to the shared token instead and lets the caller persist.
- **`STREAMING_SCOPES` is broad on purpose.** Narrowing it to the playback
  scopes made the shared fallback silently under-privileged: search kept working
  (no scope needed) while library and player calls returned a bare 403.
- **An empty refresh token must be stored as absent.** `Some("")` makes Spotify
  answer `invalid_request: refresh_token must be supplied`, failing every
  restore until the file is deleted by hand.
- **Only `invalid_grant` justifies deleting tokens.** Clearing on any failure
  meant a startup network blip logged the user out.
- **Refresh-token rotation is handled** in both the refresher and the restore
  path — skipping it breaks login on the *next* launch, not this one.
- **The Premium check runs before librespot starts**, so a free account gets a
  clear message rather than a silent failure deep in the audio pipeline. It is a
  plain read of the account's own stated plan — nothing is spoofed.
- **That check is the app's most rate-limit-exposed call.** [[commands.rs]]
  persists tokens before the gate so a 429 retry can be silent. See
  [[rate-limiting]].
- **Only the Web API token is refreshed here.** librespot's `Session` maintains
  its own connection and internal token provider once connected.
- `MIN_REFRESH_DELAY` guards a real failure mode: without a floor, an expired
  `expires_at` yields a zero sleep and the loop would hammer the token endpoint.
- **Password login is not implementable.** `Credentials::with_password` exists
  in librespot 0.8 and compiles, but Spotify disabled it server-side in July
  2024. See [[known-limitations]].

## See also

[[auth-and-tokens]] · [[rate-limiting]] · [[known-limitations]] ·
[[commands.rs]] · [[lib.rs]] · [[state.rs]] · [[webapi.rs]] · [[error.rs]] ·
[[player.rs]] · [[Login.svelte]] · [[backend-rust]] · [[MOC]]
