---
tags: [concept, limitations]
---
# Known limitations

Deliberate gaps, each with its cause. Most are upstream constraints rather than
bugs to fix in this codebase.

## Not supported at all

### Username / password login
Spotify **removed password authentication server-side at the end of July 2024**.
`Credentials::with_password` still exists in librespot 0.8's API and compiles
without warning, but the server answers `Bad credentials` every time. The
function is a leftover.

OAuth is the only route. The user still types their password — into Spotify's
own page, in the browser. What is lost is the in-app form, not account access.
ncspot has no password path either, which is part of why it "always works":
there is no fallback to hide behind. See [[auth-and-tokens]].

### Jams
Spotify's real-time collaborative listening. **No public Web API exists** and
librespot implements nothing for it. Omitted entirely rather than mocked with
a disabled button.

### Blends — read/play only
An existing Blend is an ordinary playlist: it appears in [[library.rs]]'s
playlist list and plays like any other. **Creating** a Blend or inviting a
participant uses a private, undocumented endpoint and is not implemented.

### Lyrics
Spotify has no public lyrics endpoint, and librespot 0.8 has no supported
lyrics API. Rustify therefore uses the separate documented LRCLIB API through
[[lyrics.rs]]. Synchronized LRC is preferred and plain text is the fallback.
Coverage and timing accuracy depend on LRCLIB; missing entries are reported as
unavailable and never fabricated. See [[2026-08-capability-audit]].

### Free-tier playback
librespot cannot play the ad-supported tier. Enforced early with a clear
message — see [[auth-and-tokens]].

## API-limited

### Search returns at most 10 per type
`/search` caps `limit` at **10** (default 5) — far below the 50 most other
endpoints allow, and exceeding it is a hard `400 Invalid limit`, not a silent
truncation. `search::MAX_SEARCH_LIMIT` encodes the ceiling. Unrelated to the
50s in [[library.rs]], which those endpoints do accept.

### Queue reorder / remove
`POST /me/player/queue` appends. The Web API offers **no reorder or remove
operation**. [[queue.rs]] can only read and append; jumping into the queue is
implemented as replaying it as an explicit track list from the chosen entry.

### Queue reflects the active device
`GET /me/player/queue` returns the **active** device's queue, which may be
another device's after a transfer.

### Artist popularity rankings
Spotify removed `/artists/{id}/top-tracks` for Development Mode apps in
February 2026. [[ArtistView.svelte]] instead labels and renders a deterministic
playable sample from the artist's recent releases. It does not call that sample
"Popular" or imply Spotify ranking.

### Algorithmic mixes and recommendations
Spotify's Recommendations endpoint and access to Spotify-owned algorithmic or
editorial playlists are not available to this Development Mode integration.
Home provides two truthful alternatives: a mix built from `/me/top/tracks`,
and recent releases from `/me/top/artists`. Neither is presented as a
Spotify-authored Daily Mix or recommendation shelf.

### Album rows have no cover art
`/albums/{id}/tracks` returns *simplified* track objects with no nested album,
so `imageUrl` is null on every row. [[AlbumView.svelte]] shows the art in the
header instead.

## Incomplete

### Transport controls do not drive remote devices
`play`, `pause`, `next`, `seek`, `set_volume` and friends all go through
`Spirc`, which controls **this app's player only**. While another device is
active, the UI reflects its state (via the poller) but the buttons do not
command it — Spotify wants `PUT /me/player/play` and siblings for that.

Playing something from Rustify activates this device first, so the controls work
from that point on. Driving a remote device as a true remote is unimplemented.

## Shared-quota rate limiting

**Observed in practice, not theoretical.** Without a private `RUSTIFY_CLIENT_ID`,
Web API traffic uses librespot's built-in client ID, shared by every
librespot-based client and metered per client ID over a rolling 30-second
window. A 429 can arrive on a first request with no prior usage.

Setting a private client ID fixes it for Web API calls; playback necessarily
stays on the shared ID. [[rate-limiting]] covers the full picture.

## Implementation heuristics

### `is_active_device` is inferred
librespot exposes no explicit flag. [[player.rs]] infers it from player events,
and the remote poller clears it when `/me/player` reports another device.
Reliable in practice, but a heuristic. See [[state-and-events]].

### Remote state is up to 5 s stale
The poller runs on a fixed 5 s interval, so a track change on another device can
take that long to appear. A deliberate trade against quota consumption.

### The position ticker can drift
[[store.svelte.ts]] ticks position locally at 1 Hz between backend updates. The
backend now anchors position with a timestamp and recomputes it on every
snapshot, so a snapshot can no longer *reset* the clock — but sub-second drift
between corrections remains. A deliberate CPU trade-off.

### Media keys are best-effort
Media keys are a globally exclusive OS resource. If another player already
holds them, [[media_keys.rs]] logs a warning and continues rather than failing
startup — so they can silently not work. Check the log before assuming a bug.

## Scope decisions

### Windows only
No cross-platform abstractions. The build targets `x86_64-pc-windows-msvc`;
`bundle.targets` is `nsis`. Icon generation produced iOS/Android assets as a
side effect of `tauri icon` — they are unused. See [[icons]].

### No virtual scrolling
Library and artist releases append pages on demand. Very large libraries can
therefore accumulate DOM nodes.

### Search results are not paginated
[[Search.svelte]] requests a single page (10 per type, the API maximum).

### No offline mode
librespot caches audio (a configurable 128–8192 MB cap, 2 GB by default), but
there is no offline UI; Spotify metadata and LRCLIB lyrics require network.

## Verification status

Exercised on 2026-08-18 against a live Premium account: two-role OAuth restore,
passive remote playback state, search, Home history/top-items/release shelves,
artist identity/tracks/releases/pagination, LRCLIB synchronized lyrics, track
and album save/un-save with server verification and restoration, profile,
settings persistence, and responsive setup/player geometry. Device transfer
was not performed because it would interrupt the user's active remote session.
See [[2026-08-capability-audit]] for commands and exact external constraints.

## See also

[[architecture]] · [[playback-and-connect]] · [[auth-and-tokens]] ·
[[rate-limiting]] · [[state-and-events]] · [[external-dependencies]] ·
[[README.md]] · [[MOC]]
