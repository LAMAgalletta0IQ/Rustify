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

### Jams — moved to "implemented", not omitted
> Until 2026-08 this said Jam was omitted entirely because no public Web API
> exists for it. That premise was correct but the conclusion wasn't the only
> option: **no public Web API** doesn't mean **no implementation** — Spotify's
> own official clients reach it through the private `social-connect/v2`
> service, using credentials librespot's own authenticated session already
> holds (no separate impersonation or scraping). Rustify now does the same.
> See `README_jams.md` for the endpoints, the bearer-token gotchas, and
> current status; DJ narration audio and Gabo telemetry upload remain
> genuinely blocked for reasons documented below, not because "no public API"
> was treated as the end of the investigation.

### Blends — read/play only
An existing Blend is an ordinary playlist: it appears in [[library.rs]]'s
playlist list and plays like any other. **Creating** a Blend or inviting a
participant uses a private, undocumented endpoint and is not implemented.

### Lyrics — first-party now, LRCLIB is the fallback
> Until 2026-08 this said Rustify used only LRCLIB, since Spotify has no
> *public* lyrics endpoint. That's still true of the public Web API, but
> Spotify's own first-party `color-lyrics` SpClient endpoint (authenticated,
> same session credentials as Jam/DJ/Home) is reachable and now the primary
> source: typed timing/language/RTL/highlight colors, bounded caching,
> request de-duplication. LRCLIB is the fallback when a track has no
> first-party sync data. Missing entries are still reported as unavailable
> and never fabricated. See [[2026-08-capability-audit]].

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
February 2026.
> Until 2026-08 [[ArtistView.svelte]]'s only option was a deterministic
> playable sample built from the artist's recent releases (5 albums, then
> every track of each — which was also a real N+1 request-storm contributor
> to artist-page rate limiting). `get_artist_overview` now sources real
> Spotify-ranked top tracks from Pathfinder's `queryArtistOverview`
> (`discography.topTracks`) — the same call already used for concerts and
> monthly listeners, so this costs nothing extra over the wire — falling back
> to the old REST reconstruction only if that field comes back empty, and
> further to REST-only if Pathfinder fails outright. The page still doesn't
> pretend these are a browsable "Popular" context (Spotify's public surface
> doesn't expose one), just an accurate ranked list instead of a synthesized
> sample.

### Algorithmic mixes and recommendations — now sourced from Pathfinder
> Until 2026-08 this said Spotify's Recommendations endpoint and
> Spotify-owned algorithmic/editorial playlists were unavailable to a
> Development Mode integration, so Home only ever showed Rustify-built
> substitutes (`/me/top/tracks` mixes, `/me/top/artists` releases) and
> deliberately never used the real names. That constraint was specific to the
> **public Web API**; Spotify's own web player reaches Daily Mix, Discover
> Weekly, Release Radar and the rest of the personalized-shelf family through
> Pathfinder (`api-partner.spotify.com`, GraphQL, persisted queries) using
> the same first-party session credentials as Jam/DJ/lyrics. `spotify/home.rs`
> now classifies and surfaces these by real name, semantically (not by
> matching a localized title). The "For you" view (`/me/top` data, arranged by
> Rustify, never labeled as official) still exists as a fallback for accounts
> or regions where Pathfinder's personalized shelves come back empty.
>
> Getting from a Home card to a *playable tracklist* needed a second,
> related fix: the public REST `/playlists/{id}/items` 404s specifically for
> these generated-playlist ids (they're not real playlist objects exposed to
> third-party apps there), even though the id looks like an ordinary
> `spotify:playlist:` URI everywhere else. `get_playlist_tracks` now retries
> through Pathfinder's `fetchPlaylistContents` operation on exactly that 404.

### Friend activity — implemented via Dealer + SpClient, not the public API
> Until 2026-08 this said Rustify showed an unconditional unavailable state
> because the public Web API has no friend-presence surface. Still true of
> that API specifically, but Spotify's own clients seed this from SpClient
> (`/presence-view/v2/init-friend-feed/{connectionId}`) and receive cheap
> per-user invalidation pushes over the same authenticated Dealer connection
> Jam/queue/remote-state already use (`hm://presence2/user/`). `friends.rs`
> implements that now. The status shown to the user is typed, not a single
> catch-all: `unavailable` is set only on an actual 403/404 (Spotify
> confirming the account/region genuinely lacks the capability); any other
> failure — expired token, rate limit, dropped Dealer connection, unexpected
> response shape — sets a distinct `failed` status worded as transient, never
> as a capability limit. That distinction was itself a bug fix: earlier code
> collapsed every failure into the same "not available" message regardless of
> cause.

### Lossless playback — plumbing exists now, protected playback is the actual blocker
> Until 2026-08 this said the player only ever requests 96/160/320 kbps lossy
> files and Settings omits Lossless entirely. `audio_capabilities.rs` now
> does real, non-cosmetic work: per-track Track v4 file-format inventory,
> format-aware `storage-resolve/v2` probing, and FLAC decoder
> availability/selectability detection, surfaced honestly in Now Playing/
> Settings (capability state, not a toggle that does nothing). What remains
> blocked, specifically, is the protected-stream key (PlayPlay) needed to
> actually decrypt a Lossless file once resolved — this project will not
> extract, leak, or bypass that, so 16-bit lossy playback is what actually
> plays regardless of what the capability state reports.

### Album rows have no cover art
`/albums/{id}/tracks` returns *simplified* track objects with no nested album,
so `imageUrl` is null on every row. [[AlbumView.svelte]] shows the art in the
header instead.

## Incomplete

### Transport controls now do drive remote devices
> Until 2026-08 this said transport controls only ever drove `Spirc` (this
> app's own player), so while another device was active the UI mirrored its
> state but the buttons were inert. `commands.rs` now checks
> `is_active_device` in every transport command (`play`, `pause`, `next`,
> `previous`, `seek`, `set_volume`, `set_shuffle`, `set_repeat`) and routes to
> the Web API (`PUT`/`POST /me/player/...`) instead of `Spirc` when this app
> isn't the active device — the gap this section used to describe.
>
> Playing something from Rustify still activates this device first, same as
> before; the difference is the controls now also work *before* that point,
> against whatever device actually is active.

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

### Remote state — event-driven now, the poller is a fallback
> Until 2026-08 the 5 s Web API poller was the *only* mechanism, so a remote
> track change could take up to 5 s to appear — a deliberate trade against
> quota. `remote_state.rs` now projects the Dealer-pushed Connect
> cluster/player/queue protobuf directly, which is effectively immediate; the
> 5 s poller still runs (unchanged interval) but only as a recovery/metadata
> fallback for whatever the event stream doesn't cover, not the primary path
> anymore. See [[state-and-events]].

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

### Search pages are 10 results per type
[[Search.svelte]] uses the API maximum of 10 per type and incrementally loads
further offset pages. Spotify provides no comparable cross-type score, so the
top result preserves the first API-ranked artist (or track).

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
