---
tags: [concept, auth]
---
# Authentication and tokens

Two logins, two client IDs, one session.

## The core idea

librespot needs credentials to stream. The Web API needs a bearer token for
metadata. These are served by **two separate OAuth authorizations**, because the
client ID that can stream and the client ID that should carry Web API traffic
are not the same one:

```
                Spotify desktop client ID              your own client ID
                (SessionConfig::default)               (RUSTIFY_CLIENT_ID)
                        │                                      │
                   STREAMING_SCOPES                      WEBAPI_SCOPES
                        │                                      │
                        ▼                                      ▼
        Credentials::with_access_token()          Authorization: Bearer …
             (librespot Session)                    (Spotify Web API)
```

This mirrors [ncspot](https://github.com/hrkfdn/ncspot), which uses the same
split for the same reason — see `SPOTIFY_CLIENT_ID` / `NCSPOT_CLIENT_ID` in its
`authentication.rs`.

**Cost:** the browser opens twice on an interactive login. [[Login.svelte]] says
so explicitly when a private client ID is configured.

**When no private client ID is set**, a single login serves both roles and the
browser opens once — see *The shared fallback* below.

## Why the split

The desktop client ID's Web API quota is pooled across **every librespot-based
client in the world**, so a 429 can land on a first request the user never made.
Moving Web API traffic onto a self-registered app gives it a private quota. See
[[rate-limiting]].

The reverse move is not available: self-registered apps are generally refused the
`streaming` scope, so playback must stay on the desktop ID. That is why
`streaming_client_id()` is a hardcoded constant while `webapi_client_id()` is
configurable.

## Client IDs and redirects

| Role | Client ID | Redirect | Configurable |
| --- | --- | --- | --- |
| Streaming | `SessionConfig::default().client_id` | `http://127.0.0.1:8898/login`, falling back to an ephemeral port if busy | No |
| Web API | `RUSTIFY_CLIENT_ID` from `.env` | `http://127.0.0.1:8899/login`, fixed | Yes |

The asymmetry in the redirect ports is deliberate. The desktop ID accepts any
loopback port, so a port held by a stale process is recoverable. A
self-registered app must declare its redirect URI **exactly** in the Spotify
dashboard, so that one cannot be chosen at random.

Registering the app: dashboard → Create app → add `http://127.0.0.1:8899/login`
→ put the Client ID in `.env`. A Client ID is not a secret; this flow is PKCE
and uses no client secret.

## Scopes

`STREAMING_SCOPES` is deliberately the **full union**, not just `streaming`.

That looks redundant when the split is active, and it is — but this token is the
fallback whenever a private Web API token is unavailable. Narrowing it to the
playback scopes made that fallback silently under-privileged: search still
worked (it needs no scope) while every library and player call returned a bare
403. ncspot requests the same broad set against this client ID.

| Group | Scopes |
| --- | --- |
| Playback | `streaming` (streaming login only) |
| Profile | `user-read-private`, `user-read-email` |
| Connect | `user-read-playback-state`, `user-modify-playback-state`, `user-read-currently-playing` |
| Library | `user-library-read`, `user-library-modify`, `playlist-read-private`, `playlist-read-collaborative`, `playlist-modify-private`, `playlist-modify-public`, `user-follow-read`, `user-top-read`, `user-read-recently-played` |

`WEBAPI_SCOPES` is the same list minus `streaming`.

Some scopes are requested ahead of the features that need them
(`playlist-modify-*`, `user-follow-read`, `user-read-recently-played` are
currently unused), so adding those features later needs no re-consent.

## The shared fallback

`SessionTokens::shared()` points both roles at the streaming token. Reached when:

- no `RUSTIFY_CLIENT_ID` is configured — the normal single-login path; or
- one is configured but no Web API refresh token is stored (logged at `warn`); or
- the Web API refresh fails during a restore (logged at `warn`).

That third case must **not** propagate as an error. The streaming refresh has
already succeeded by then, and Spotify rotates refresh tokens on use — so the
stored streaming token is dead and its replacement exists only in memory.
Failing there would drop the rotation and lock the user out on the next launch.

## The Premium gate

librespot cannot play the free, ad-supported tier. Rather than let that fail
somewhere deep in the audio pipeline, `fetch_profile_require_premium`
([[auth.rs]]) reads `GET /v1/me` and checks `product`:

- `"premium"` → proceed.
- anything else → `AppError::PremiumRequired(product)`.

This runs **before** librespot starts, so a free account gets a clear message
instead of silence. [[Login.svelte]] branches on the `kind` field to show a
dedicated explanation.

> **The gate is the app's most rate-limit-exposed call** — one `GET /v1/me` per
> login. A 429 there once failed a login whose OAuth had already succeeded,
> which is why refresh tokens are persisted *before* the gate: a retry can then
> go through `restore_session` with no browser. See [[rate-limiting]].

This is a plain read of the account's own stated plan. Nothing is spoofed or
bypassed — it exists to produce a good error message, not to gate anything.

## Token lifecycle

```
                 ┌──────────────────────────────┐
   first run ──► │ interactive_login()          │  browser opens ×2
                 │  1. streaming (desktop ID)   │  (×1 if no private ID)
                 │  2. web api  (private ID)    │
                 └──────────────┬───────────────┘
                                ▼
                 ┌──────────────────────────────┐
                 │ establish()                  │
                 │  1. save both refresh tokens │
                 │  2. Premium check (web token)│
                 │  3. tokens.set(web access)   │
                 │  4. start_session (streaming)│
                 │  5. spawn_refresher          │
                 │  6. spawn_remote_poller      │
                 │  7. tear down old session    │
                 └──────────────┬───────────────┘
                                ▼
                 ┌──────────────────────────────┐
                 │ spawn_refresher (loop)       │
                 │  sleep → refresh → set →     │
                 │  persist if rotated          │
                 └──────────────────────────────┘

  later runs ──► restore_session() → restore_login() → establish()
                 (no browser; refreshes both tokens)
```

### The refresher

Access tokens last ~1 hour. Without renewal, every Web API call — library,
search, devices, queue, and the metadata behind the now-playing bar — would
start returning 401 after an hour of uptime.

- Wakes `REFRESH_MARGIN` (5 min) before `expires_at`, floored at
  `MIN_REFRESH_DELAY` (30 s) so an already-expired token cannot spin.
- Refreshes whichever token is serving the Web API role, under the client ID
  that issued it.
- Persists a rotated refresh token into the correct field, preserving the other.
- **Retries rather than ending the session** on failure; the likely cause is a
  transient network error, not a revoked grant.
- Aborted on logout via the `JoinHandle` stored in `SpotifySession`.

Only the *Web API* token is on this schedule. librespot's `Session` maintains its
own connection and internal token provider once connected.

## Storage

`tokens.json` in the Tauri app data dir holds only refresh tokens — access
tokens are never written to disk.

```jsonc
{
  "refresh_token": "…",            // streaming (desktop ID)
  "webapi_refresh_token": "…"      // absent unless the split is active
}
```

`webapi_refresh_token` is `#[serde(default)]`, so files written before the split
still load. An empty string is stored as **absent**, never as `Some("")` —
Spotify rejects a blank refresh token with `invalid_request: refresh_token must
be supplied`, which would fail every restore until the file was deleted by hand.

**Writing is a merge, not a replacement.** `save_stored_tokens` reads the
existing file first and keeps the stored `webapi_refresh_token` whenever the
session being saved has none. A session that fell back to the shared token
legitimately carries `None`, and writing that through would delete a valid
credential. Deliberate clearing goes through `clear_stored_tokens`, which
deletes the file, so preserving on `None` cannot strand a dead token.

**Spotify only sends `refresh_token` back when it rotates one.** An omitted
field means "keep the one you have", not "you have none" — but it deserialises
to the empty string, which reads as the latter. `restore_login` copies the
outgoing token back over an empty response field before building
`SessionTokens`; without that, `stored()` reported the session as having no Web
API credential at all.

librespot separately caches its own credentials and up to 2 GB of audio under
`<app data>/cache`.

> The Tauri `identifier` feeds the app data dir path, so changing it orphans
> `tokens.json` and forces one fresh login. See [[build-and-config]].

## Deleting stored tokens

Only `invalid_grant` / `invalid_client` — Spotify saying the grant itself is
dead — justifies deleting `tokens.json` (`auth::is_grant_rejected`). Everything
else is transient.

Clearing on *any* restore failure meant a network blip at startup logged the
user out, which fired intermittently whenever the app started faster than the
network came up.

## Failure modes

| Situation | Behaviour |
| --- | --- |
| No stored token | Logged-out state, login screen. Not an error |
| Stored token rejected (`invalid_grant`) | File deleted, login screen. Logged as a warning |
| Restore fails transiently | **Tokens kept**, login screen, retried next launch |
| Web API refresh fails mid-restore | Degrades to the shared token; rotation preserved |
| Free account | `PremiumRequired` with the plan name |
| **Rate limited on `/me`** | `RateLimited` with `Retry-After`; UI counts down and silently retries |
| Logout | Spirc shut down, refresher + poller aborted, token cleared, file deleted |
| Re-login without logout | Old session shut down, its tasks aborted first |

## Not supported: username and password

Spotify removed password authentication server-side at the end of July 2024.
`Credentials::with_password` still exists in librespot 0.8's API and compiles
fine, but the server answers `Bad credentials` every time. OAuth is the only
route; the user still types their password, into Spotify's own page. ncspot has
no password path either. See [[known-limitations]].

## See also

[[architecture]] · [[data-flow]] · [[state-and-events]] · [[rate-limiting]] ·
[[known-limitations]] · [[auth.rs]] · [[commands.rs]] · [[lib.rs]] ·
[[Login.svelte]] · [[error.rs]] · [[MOC]]
