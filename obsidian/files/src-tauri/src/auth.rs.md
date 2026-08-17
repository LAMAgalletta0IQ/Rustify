---
tags: [file, backend, auth, rust]
---
# `src-tauri/src/auth.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **199 lines**

## Purpose

Everything about identity: the OAuth flow, scope list, refresh-token
persistence, the background token refresher, and the Premium check. Conceptual
overview in [[auth-and-tokens]].

## Key items

### Constants
- `REDIRECT_URI = "http://127.0.0.1:8898/login"` — loopback address librespot's
  OAuth helper serves, registered against Spotify's desktop client ID.
- `SCOPES: &[&str]` — 15 scopes, the union of librespot's streaming needs and
  the Web API's. Some are requested ahead of the features that use them so
  adding those later needs no re-consent.
- `REFRESH_MARGIN = 5 min` — how early to renew.
- `MIN_REFRESH_DELAY = 30 s` — floor, so an already-expired token cannot spin.

### `struct StoredTokens { refresh_token: String }`
What is persisted. **The access token is never written to disk.**

### Token file helpers
`load_stored_tokens`, `save_stored_tokens`, `clear_stored_tokens` — read/write
`tokens.json` in the Tauri app data dir. Loading returns `Option`, so a missing
or corrupt file is simply "not logged in" rather than an error.

### `fn build_client(client_id) -> OAuthClient`
Builds an `OAuthClientBuilder` with `.open_in_browser()` and a custom success
message shown in the browser tab after approval.

### `async fn interactive_login(client_id) -> OAuthToken`
Opens the system browser and waits for the loopback redirect.

### `async fn refresh_login(client_id, refresh_token) -> OAuthToken`
Silent renewal. Used both at startup by `restore_session` and by the refresher.

### `const CLIENT_ID_ENV: &str = "SPOTIFY_RUST_CLIENT_ID"`
### `fn default_client_id() -> String`
Returns the env-var override when set and non-blank (logging that it did),
otherwise `SessionConfig::default().client_id` — Spotify's own desktop client
ID, shipped with librespot. Using a first-party ID is what permits the broad
scope list.

> **The override is a last resort, not a tuning knob.** The default ID is shared
> by every librespot-based client, so its Web API quota is consumed globally —
> the cause of spurious 429s. But a newly registered app starts in Spotify's
> *restricted* quota mode and often cannot obtain the `streaming` scope at all,
> which would break playback outright. See [[rate-limiting]].

### `fn spawn_refresher(...) -> JoinHandle<()>`
The background loop. Per iteration:
1. Sleep until `expires_at − REFRESH_MARGIN`, floored at `MIN_REFRESH_DELAY`.
2. `refresh_login()`.
3. On success: `tokens.set(new_access_token)`, update `expires_at`, and persist
   the refresh token **if it rotated**.
4. On failure: log a warning and retry after the margin + floor.

### `async fn fetch_profile_require_premium(api, token) -> AuthState`
`GET /v1/me`, then rejects any `product` other than `"premium"` with
`AppError::PremiumRequired`. Returns a populated `AuthState` on success,
picking the first profile image as the avatar.

## Inputs / outputs / side effects

- **Network:** Spotify OAuth endpoints; `GET /v1/me`.
- **Filesystem:** reads/writes/deletes `tokens.json`.
- **Spawns** a long-lived tokio task (the refresher).
- **Opens the system browser** during interactive login.

## Dependencies

**Imports:** `librespot_oauth`, `librespot::core::config::SessionConfig`,
`serde`, `tokio::time`, [[state.rs]] (`AuthState`, `TokenStore`),
[[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]]

## Notable logic / gotchas

- **Refresh-token rotation is handled.** Spotify may return a new refresh token;
  the old one then stops working. The code compares and persists on change —
  skipping this would break login on the *next* launch, not this one.
- **The refresher retries rather than ending the session.** A transient network
  error should not log the user out.
- **The Premium check runs before librespot starts**, so a free account gets a
  clear message instead of a silent failure deep in the audio pipeline. The
  source comment is explicit that this is a plain read of the account's own
  stated plan — nothing is spoofed.
- **That check is also the app's most rate-limit-exposed call.** It is a
  `GET /v1/me` on every login, and a 429 there once failed a login whose OAuth
  had already succeeded. [[commands.rs]] now persists the refresh token before
  the gate so a retry can be silent. See [[rate-limiting]].
- **Only the Web API token is refreshed here.** librespot's `Session` maintains
  its own connection and internal token provider once connected.
- `MIN_REFRESH_DELAY` guards a real failure mode: without a floor, an expired
  `expires_at` yields a zero sleep and the loop would hammer the token endpoint.

## See also

[[auth-and-tokens]] · [[rate-limiting]] · [[commands.rs]] · [[state.rs]] ·
[[webapi.rs]] · [[error.rs]] · [[player.rs]] · [[Login.svelte]] ·
[[backend-rust]] · [[MOC]]
