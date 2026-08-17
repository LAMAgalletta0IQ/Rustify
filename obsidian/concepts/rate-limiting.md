---
tags: [concept, webapi, auth]
---
# Rate limiting (HTTP 429)

A real, recurring operational issue for this app — not a hypothetical. Worth
understanding before assuming a 429 is a bug in the code.

## Why it happens with no usage at all

Spotify computes Web API quota **per client ID**, over a rolling 30-second
window. This app uses librespot's built-in client ID (Spotify's own desktop ID),
which is **shared by every librespot-based client in the world**.

The consequence: the quota is consumed *globally*, by other people's
applications. A 429 can arrive on the very first request after launching the
app, having done nothing.

Observed in practice: a fresh login returned 429 with `Retry-After: 58` on
`GET /v1/me`, the Premium check.

## Why registering your own app is not an obvious fix

The intuitive fix — register a Spotify Developer app for a private quota — is
frequently **worse**:

| | librespot's default ID | A newly registered ID |
| --- | --- | --- |
| Quota mode | Extended (grandfathered, predates the Nov 2024 API changes) | **Restricted default mode** |
| 429 / 403 frequency | Lower | Commonly higher |
| `streaming` scope | Works | **Often refused (403)** — would break playback |

That last row is the dangerous one. Several librespot issues report custom
client IDs failing to obtain `streaming` when combined with user scopes, which
breaks audio entirely rather than just metadata.

The override exists in [[auth.rs]] (`SPOTIFY_RUST_CLIENT_ID`) but is a
last resort, not a recommended default.

## This app does not poll — keep it that way

The 30-second window is measured **per client ID**, so every request counts
against the same shared pool. Other librespot-based clients that poll
`/v1/me/player` every few seconds burn that pool on our behalf — a documented
pattern in at least one other Spotify client, and a plausible reason this app
sees 429s having issued a single request.

We cannot control other clients. We *can* avoid being one of them. Current
request triggers, all event- or user-driven:

| Endpoint | Where | Trigger |
| --- | --- | --- |
| `GET /me/player/devices` | [[connect.rs]] | Device picker opened or ⟳ pressed |
| `PUT /me/player` | [[connect.rs]] | User transfers playback |
| `GET /me/player/queue` | [[queue.rs]] | Track change, while now-playing is open |
| `GET /tracks/{id}` | [[player.rs]] | New track — cached per URI |
| `GET /me/tracks/contains` | [[TrackList.svelte]] | Track list mounted |

The only recurring timers are local and issue **no** network traffic: the 1 Hz
position ticker in [[store.svelte.ts]], the 300 ms search debounce in
[[Search.svelte]], and the login countdown in [[Login.svelte]].

> **Do not add a status poll.** Playback state arrives from librespot's
> `PlayerEvent` stream ([[state-and-events]]), which is push-based and free. A
> `/me/player` poll would duplicate data the app already has *and* consume the
> shared quota.

## How the code handles it

Three layers, in order:

**1. Detection — [[webapi.rs]]**
A 429 is caught before any other status handling and returned as
`AppError::RateLimited { retry_after }`, carrying Spotify's own header value.
Previously the header was discarded and an empty body rendered as a bare
`"429 Too Many Requests: "`.

**2. Absorption — [[webapi.rs]] `get()`**
GETs retry automatically through short windows (≤ `MAX_AUTO_RETRY_SECS` = 8s,
up to `MAX_RETRIES` = 2), honouring `Retry-After` or falling back to
`1 << attempt`. **Only GETs** — repeating a POST could double-queue a track.

**3. Surfacing — [[Login.svelte]]**
Longer windows reach the UI, which shows a live countdown and retries itself
when the window expires (+2s slack, because retrying on the exact boundary
tends to be refused again).

## The login path specifically

The Premium gate is a `GET /v1/me`, so a rate limit there once failed an
otherwise completely successful login — OAuth had already succeeded.

Two changes address it:

- **[[commands.rs]] `establish()` persists the refresh token *before* the
  Premium gate.** The OAuth flow has already completed at that point, so a
  retry can go through `restore_session` silently.
- **[[Login.svelte]]'s auto-retry passes `silent = true`**, using
  `restoreSession()` rather than `login()`. Without this, the countdown would
  **reopen the browser every 60 seconds** — worse than the original failure.

## Diagnosing a 429

| Symptom | Meaning |
| --- | --- |
| Heading "Rate limited by Spotify" | Our Web API call. `kind === "RateLimited"` |
| Heading "Login failed", 429 in the text | The OAuth token exchange — a different endpoint, different fix |
| Clears after one countdown | Normal transient pool contention |
| Countdown loops repeatedly | Pool genuinely saturated; stop retrying and wait several minutes |
| Persists after 10–15 quiet minutes | Something else. Only now consider the client ID override |

Each retry adds to the contention, so repeated hammering makes it worse.

## A self-inflicted variant

Worth knowing because it was a real bug: `establish()` used to overwrite
`state.spotify` without aborting the previous session's refresher. **Dropping a
tokio `JoinHandle` detaches the task rather than cancelling it**, so two
refreshers would hit the token endpoint on independent schedules. Fixed by
tearing the old session down explicitly — see [[commands.rs]].

## See also

[[auth-and-tokens]] · [[webapi.rs]] · [[error.rs]] · [[commands.rs]] ·
[[auth.rs]] · [[Login.svelte]] · [[external-dependencies]] ·
[[known-limitations]] · [[MOC]]
