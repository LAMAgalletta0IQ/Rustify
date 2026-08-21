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

### `fn webapi_client_id(data_dir: &Path) -> AppResult<String>`
Resolves, in order: the `RUSTIFY_CLIENT_ID` env var (packager/dev override),
then `settings.json` in `data_dir` (written by `set_client_id` in
[[commands.rs]] via the Setup screen), then `Err`. There is no built-in
fallback — see the gotcha below.

### `struct Settings` / `load_settings` / `save_settings`
`settings.json` stores the Client ID, private last player volume, reduced motion
(false), and cache cap (2048 MB). Serde defaults migrate older Client-ID-only
files. The document survives logout and every update merges rather than
overwriting the Client ID.

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

**`save_stored_tokens` is a merge, not a plain write.** It reads the existing
file first and, when the session being saved has `webapi_refresh_token: None`,
keeps whatever was already on disk. A session that fell back to the shared
token legitimately has no Web API refresh token, and writing that `None`
straight out would delete a perfectly good stored credential. Deliberate
clearing goes through `clear_stored_tokens`, which removes the file outright,
so preserving on `None` cannot strand a dead token.

### `async fn interactive_login(data_dir: &Path) -> SessionTokens`
Opens the browser for the streaming authorization, then **again** for the Web
API one under `webapi_client_id(data_dir)`. Propagates that call's `Err` if no
Client ID is configured — in practice unreachable via the UI, since
[[commands.rs]]'s `login` command runs after Setup has already gated on it.

### `async fn restore_login(&StoredTokens, data_dir: &Path) -> SessionTokens`
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
4. On transient failure: log and retry after the margin + floor.
5. On `invalid_grant`/`invalid_client`: clear rejected tokens, shut down the
   session, reset auth/playback, emit logged-out state, and stop the task.

### `async fn fetch_profile_require_premium(api, token) -> AuthState`
`GET /v1/me`, then rejects a present non-premium `product` with
`AppError::PremiumRequired`. Spotify may omit `product` for newer Development
Mode apps; in that case librespot's streaming handshake is authoritative.
Returns identity/avatar and preserves an absent product as `None`.

## Inputs / outputs / side effects

- **Network:** Spotify OAuth endpoints (two client IDs); `GET /v1/me`.
- **Filesystem:** reads/writes/deletes `tokens.json`; reads/writes `settings.json`.
- **Environment:** reads `RUSTIFY_CLIENT_ID` (override only — see gotchas).
- **Binds** loopback ports 8898 / 8899 during login.
- **Spawns** a long-lived tokio task (the refresher).
- **Opens the system browser** — twice when the split is active.

## Dependencies

**Imports:** `librespot_oauth`, `librespot::core::config::SessionConfig`,
`serde`, `std::net::TcpListener`, `tokio::time`, [[state.rs]] (`AuthState`,
`TokenStore`), [[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]], [[lib.rs]]

## Notable logic / gotchas

- **There is no built-in Client ID fallback, on purpose.** Earlier versions
  baked one in (`CLIENT_ID_FALLBACK`) so an unconfigured install still "just
  worked" — but that meant sharing *the app author's* quota, which does not
  scale past one install. `webapi_client_id` now errors when unconfigured, and
  [[commands.rs]]/[[App.svelte]] turn that into the first-run Setup screen
  rather than a degraded default. See [[auth-and-tokens]].
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
- **Spotify only returns `refresh_token` when it actually rotates one.** An
  omitted field means "keep using the one you have", **not** "you no longer
  have one" — but it deserialises to the empty string, which reads as the
  latter and made `stored()` report the session as having no Web API credential
  at all. `restore_login` now copies the incoming token back over an empty
  response field before building `SessionTokens`.
- **Losing the Web API refresh token is self-perpetuating, which is why
  `save_stored_tokens` preserves it.** The real failure ran: one 429 on `/me`
  at startup → `restore_login` degrades to the shared token → the save that
  follows writes `None` and wipes the private refresh token → every later
  launch is stuck on librespot's globally-pooled quota → more 429s → repeat. A
  manual re-login fixed it only until the next 429. Note how many of the
  individually-correct behaviours above combined to produce it: degrading
  rather than erroring, and persisting before the Premium gate. See
  [[rate-limiting]].
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
[[player.rs]] · [[Login.svelte]] · [[Setup.svelte]] · [[backend-rust]] · [[MOC]]
