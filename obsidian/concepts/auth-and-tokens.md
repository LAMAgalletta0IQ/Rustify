---
tags: [concept, auth]
---
# Authentication and tokens

One login, two consumers, one refresh path.

## The core idea

librespot needs credentials to stream. The Web API needs a bearer token for
metadata. **Both are satisfied by the same OAuth access token**, so the user
logs in once:

```
librespot_oauth  ──►  OAuthToken { access_token, refresh_token, expires_at }
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
Credentials::with_access_token()    Authorization: Bearer …
   (librespot Session)                 (Spotify Web API)
```

The alternative — a separate PKCE flow for the Web API — would mean two browser
logins for one app. See [[auth.rs]].

## Client ID and redirect

`SessionConfig::default().client_id` is Spotify's own desktop client ID, which
ships with librespot. The redirect is `http://127.0.0.1:8898/login`, served by
librespot's OAuth helper on loopback.

Using a first-party client ID is what allows the broad scope list below.

It is also the source of the app's 429s: that ID is shared by **every**
librespot-based client, so its Web API quota is consumed globally. Overriding it
with `SPOTIFY_RUST_CLIENT_ID` is possible but usually counterproductive — a
newly registered app starts in restricted quota mode and often cannot obtain
the `streaming` scope. See [[rate-limiting]].

## Scopes

The union of what librespot needs to stream and what the Web API needs:

| Group | Scopes |
| --- | --- |
| Playback | `streaming` |
| Profile | `user-read-private`, `user-read-email` |
| Connect | `user-read-playback-state`, `user-modify-playback-state`, `user-read-currently-playing` |
| Library | `user-library-read`, `user-library-modify`, `playlist-read-private`, `playlist-read-collaborative`, `playlist-modify-private`, `playlist-modify-public`, `user-follow-read`, `user-top-read`, `user-read-recently-played` |

Some are requested ahead of the features that need them
(`playlist-modify-*`, `user-follow-read`, `user-read-recently-played` are
currently unused), so adding those features later needs no re-consent.

## The Premium gate

librespot cannot play the free, ad-supported tier. Rather than let that fail
somewhere deep in the audio pipeline, `fetch_profile_require_premium`
([[auth.rs]]) reads `GET /v1/me` and checks `product`:

- `"premium"` → proceed.
- anything else → `AppError::PremiumRequired(product)`.

This runs **before** librespot starts, so a free account gets a clear message
instead of silence. [[Login.svelte]] branches on the `kind` field to show a
dedicated explanation.

> **The gate is also the most rate-limit-exposed call in the app** — one
> `GET /v1/me` per login. A 429 there once failed a login whose OAuth had
> already succeeded, which is why the refresh token is now persisted *before*
> the gate: a retry can then go through `restore_session` with no browser. See
> [[rate-limiting]].

This is a plain read of the account's own stated plan. Nothing is spoofed or
bypassed — it exists to produce a good error message, not to gate anything.

## Token lifecycle

```
                 ┌──────────────────────────────┐
   first run ──► │ interactive_login()          │  browser opens
                 └──────────────┬───────────────┘
                                ▼
                 ┌──────────────────────────────┐
                 │ establish()                  │
                 │  1. save refresh token       │
                 │  2. Premium check            │
                 │  3. tokens.set(access)       │
                 │  4. start_session (librespot)│
                 │  5. spawn_refresher          │
                 │  6. tear down old session    │
                 └──────────────┬───────────────┘
                                ▼
                 ┌──────────────────────────────┐
                 │ spawn_refresher (loop)       │
                 │  sleep → refresh → set →     │
                 │  persist if rotated          │
                 └──────────────────────────────┘

  later runs ──► restore_session() → refresh_login() → establish()
                 (no browser)
```

### The refresher

Access tokens last ~1 hour. Without renewal, every Web API call — library,
search, devices, queue, and the metadata behind the now-playing bar — would
start returning 401 after an hour of uptime.

- Wakes `REFRESH_MARGIN` (5 min) before `expires_at`, floored at
  `MIN_REFRESH_DELAY` (30 s) so an already-expired token cannot spin.
- Persists a rotated refresh token — Spotify may issue a new one, and the old
  then stops working.
- **Retries rather than ending the session** on failure; the likely cause is a
  transient network error, not a revoked grant.
- Aborted on logout via the `JoinHandle` stored in `SpotifySession`, so it
  cannot outlive the session.

Only the *Web API* token is on this schedule. librespot's `Session` maintains
its own connection and internal token provider once connected.

## Storage

`tokens.json` in the Tauri app data dir holds only the refresh token — the
access token is never written to disk. Deleted on logout and whenever a stored
token is rejected.

librespot separately caches its own credentials and up to 2 GB of audio under
`<app data>/cache`.

## Failure modes

| Situation | Behaviour |
| --- | --- |
| No stored token | Logged-out state, login screen. Not an error |
| Stored token revoked | File deleted, login screen. Logged as a warning |
| Free account | `PremiumRequired` with the plan name |
| Refresh fails transiently | Retried; session continues |
| **Rate limited on `/me`** | `RateLimited` with `Retry-After`; UI counts down and silently retries |
| Logout | Spirc shut down, refresher aborted, token cleared, file deleted |
| Re-login without logout | Old session shut down and its refresher aborted first |

## See also

[[architecture]] · [[data-flow]] · [[state-and-events]] · [[rate-limiting]] ·
[[auth.rs]] · [[commands.rs]] · [[Login.svelte]] · [[error.rs]] · [[MOC]]
