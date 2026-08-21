---
tags: [concept, webapi, auth]
---
# Rate limiting (HTTP 429)

A real, recurring operational issue for this app — not a hypothetical. Worth
understanding before assuming a 429 is a bug in the code.

## Why it happens with no usage at all

Spotify computes Web API quota **per client ID**, over a rolling 30-second
window. librespot's built-in client ID (Spotify's own desktop ID) is **shared by
every librespot-based client in the world**.

The consequence: the quota is consumed *globally*, by other people's
applications. A 429 can arrive on the very first request after launching the
app, having done nothing.

Observed in practice: a fresh login returned 429 with `Retry-After: 58` on
`GET /v1/me`, the Premium check.

## The fix: a private client ID for Web API traffic

Setting `RUSTIFY_CLIENT_ID` in `.env` (see [[build-and-config]]) moves **all Web
API traffic** onto a self-registered app with its own quota. Playback stays on
the desktop ID. See [[auth-and-tokens]] for the mechanism.

> **This page previously argued against registering your own app.** That
> reasoning assumed one client ID had to serve both roles, which made the
> `streaming` scope a blocker. Splitting the two removes the conflict:
> `streaming` is never requested against the private ID, so it cannot be
> refused.

| | Desktop ID (playback) | Your ID (Web API) |
| --- | --- | --- |
| Quota mode | Extended, but shared globally | Restricted default, but **private** |
| 429 frequency | Can fire on a first request | Only from your own traffic |
| `streaming` scope | Works | Never requested |

The original concern — that self-registered apps are refused `streaming` — is
real, which is exactly why `streaming_client_id()` is not configurable.

**Verified in practice:** a newly registered app was granted every scope in
`WEBAPI_SCOPES` on the first authorization. Restricted default quota mode does
not appear to withhold these user scopes for the app owner's own account.

Without a private ID the app still works; it just shares the pool and is
exposed to the 429s above. [[Login.svelte]] says so on any rate-limit error, and
[[lib.rs]] logs which mode is active at startup:

```
[INFO rustify_lib] web api client id: private (from environment)
[INFO rustify_lib] web api client id: shared librespot default — see .env.example
```

## Request budget

Every request counts against the pool for its client ID. Current triggers:

| Endpoint | Where | Trigger |
| --- | --- | --- |
| `GET /me/player` | [[player.rs]] | **Poller, every 5 s — only while another device is active** |
| `GET /me/player/devices` | [[connect.rs]] | Device picker opened or ⟳ pressed |
| `PUT /me/player` | [[connect.rs]] | User transfers playback |
| `GET /me/player/queue` | [[queue.rs]] | Track change, while now-playing is open |
| `GET /tracks/{id}` | [[player.rs]] | New track — cached per URI |
| `GET /me/library/contains` | [[TrackList.svelte]], [[AlbumView.svelte]] | Saved-state decoration; 40-URI chunks |
| `PUT/DELETE /me/library` | save/remove actions | Non-idempotent UI action; never auto-retried |
| `GET /me/top/{tracks|artists}` | Home/Profile | Personalized history shelves |
| `GET /me/player/recently-played` | Home | Recent context shelf |
| `GET /search` | [[search.rs]] | 300 ms after typing stops |
| `GET /me/player/recently-played` | [[library.rs]] | Home mounted — once per visit |
| `GET /me/following?type=artist` | [[library.rs]] | Library → Artists selected, and "Load more" |

Both of the last two are **one-shot, user-triggered** fetches, not timers. Home
asks for its two shelves (`/me/playlists` and recently-played) in one
`Promise.all` on mount; the Library sections each fetch once and cache in
component state, so re-selecting a chip costs nothing.

### The one poll, and why it exists

> **This page previously said "do not add a status poll."** That held while the
> app always made itself the active device at login — playback state then came
> entirely from librespot's push-based `PlayerEvent` stream, and a poll would
> have duplicated it.
>
> Removing that takeover (see [[playback-and-connect]]) made the app a genuine
> passive Connect device, and `PlayerEvent`s only ever describe audio *this app*
> produces. With no poll, listening on a phone showed "Nothing playing".

The poll is deliberately narrow:

- **Skipped entirely while `is_active_device`** — local playback stays
  event-driven and costs nothing.
- 5 s interval: ~720 requests/hour worst case, against a private quota.
- Emits an event only when something actually changed.
- Aborted on logout with the rest of the session's tasks.

The remaining recurring timers are local and issue **no** network traffic: the
1 Hz position ticker in [[store.svelte.ts]], the 300 ms search debounce in
[[Search.svelte]], and the login countdown in [[Login.svelte]].

## How the code handles it

Three layers, in order:

**1. Detection — [[webapi.rs]]**
A 429 is caught before any other status handling and returned as
`AppError::RateLimited { retry_after }`, carrying Spotify's own header value.

**2. Absorption — [[webapi.rs]] `get()`**
GETs retry automatically through short windows (≤ `MAX_AUTO_RETRY_SECS` = 8s,
up to `MAX_RETRIES` = 2), honouring `Retry-After` or falling back to
`1 << attempt`. **Only GETs** — repeating a POST could double-queue a track.

**3. Surfacing — [[Login.svelte]]**
Longer windows reach the UI, which shows a live countdown and retries itself
when the window expires (+2s slack, because retrying on the exact boundary
tends to be refused again). The accompanying text differs depending on whether a
private client ID is configured, via `get_login_info`.

## The login path specifically

The Premium gate is a `GET /v1/me`, so a rate limit there once failed an
otherwise completely successful login — OAuth had already succeeded.

Two changes address it:

- **[[commands.rs]] `establish()` persists refresh tokens *before* the Premium
  gate.** The OAuth flow has already completed at that point, so a retry can go
  through `restore_session` silently.
- **[[Login.svelte]]'s auto-retry passes `silent = true`**, using
  `restoreSession()` rather than `login()`. Without this, the countdown would
  **reopen the browser every 60 seconds** — worse than the original failure.

## Diagnosing a 429

| Symptom | Meaning |
| --- | --- |
| Heading "Rate limited by Spotify" | Our Web API call. `kind === "RateLimited"` |
| Heading "Login failed", 429 in the text | The OAuth token exchange — a different endpoint, different fix |
| Startup log says `shared librespot default` | No private client ID; expected. Set `RUSTIFY_CLIENT_ID` |
| Log warns `using the shared token` | Split configured but degraded — log out and back in |
| Clears after one countdown | Normal transient pool contention |
| Persists on a private client ID | Genuinely our own traffic. Check for a runaway poller |
| `web api=false` in the persist log | The private refresh token is not reaching disk — the spiral below |

Since 2026-08, [[webapi.rs]] logs method, URL, status and Spotify's own message
for every failed request — start there rather than guessing.

`establish` also logs, once per login, which halves were written:

```
[INFO rustify_lib::commands] persisting tokens: streaming=true, web api=true (split=true)
```

`split` is whether the Web API client ID differs from the streaming one. If
`web api=false` while `split=true`, the private credential is being lost and
the *next* launch will silently drop to the shared quota — the failure and its
cause are a restart apart, which is precisely what made the spiral below so
hard to see.

## Self-inflicted variants

Both were real bugs, both worth knowing:

- `establish()` used to overwrite `state.spotify` without aborting the previous
  session's refresher. **Dropping a tokio `JoinHandle` detaches the task rather
  than cancelling it**, so two refreshers hit the token endpoint on independent
  schedules. Fixed by tearing the old session down explicitly.
- A degraded restore fell back to the shared token and immediately hit
  `Retry-After: 34` — a private-quota app landing on the shared pool through a
  code path that should not have been reachable. See [[auth-and-tokens]].
- **The 429 death spiral**, and the reason `save_stored_tokens` merges. One 429
  on `/me` at startup made `restore_login` degrade to the shared token; the
  save that followed wrote `webapi_refresh_token: None` and **erased the
  private refresh token**; every later launch then had no private credential,
  fell back to the globally-pooled quota, and collected more 429s — which
  re-triggered the same erasure. A manual re-login fixed it only until the next
  429.

  What makes this one worth studying is that **no individual behaviour was
  wrong**. Degrading rather than erroring on a failed Web API refresh is
  correct (rotation would otherwise be lost). Persisting before the Premium
  gate is correct (it lets a 429 retry silently). Storing an empty token as
  absent is correct. Composed, they deleted a credential on a transient
  network error. The fix was to make the write preserve what it cannot
  replace; `establish` also now logs which halves reached disk, because the
  cause and the symptom were separated by a restart.

## See also

[[auth-and-tokens]] · [[playback-and-connect]] · [[webapi.rs]] · [[error.rs]] ·
[[commands.rs]] · [[auth.rs]] · [[player.rs]] · [[Login.svelte]] ·
[[external-dependencies]] · [[known-limitations]] · [[MOC]]
