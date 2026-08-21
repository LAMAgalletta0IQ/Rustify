---
tags: [concept, audit, spotify, librespot, lyrics]
---
# 2026-08 capability and implementation audit

Verified 2026-08-18 against Rustify's pinned `librespot 0.8.0`, current Spotify
Web API documentation, Context7, a live Premium Development Mode account, and
the running Tauri application.

## Capability boundary

| Capability | Legitimate source | Final behavior |
| --- | --- | --- |
| Local audio / Connect receiver / context playback | librespot `Spirc`, `Player`, `Session` | Rust backend only |
| Search, profile, library, devices, queue, metadata | Spotify Web API | Rust `WebApi`; typed Tauri IPC |
| Recent activity | `/me/player/recently-played` | Uses `played_at` and `context`; album/playlist/artist contexts deduplicate by URI, loose tracks remain tracks |
| Personalized mix alternative | `/me/top/tracks` | Explicitly labeled as built from listening history, not Spotify editorial data |
| Discovery alternative | `/me/top/artists` + artist releases | Explicitly labeled recent releases from top artists |
| Artist tracks | Artist albums + album tracks | Labeled "Tracks from recent releases" because top-tracks was removed |
| Save/remove/check | Generic `/me/library` URI endpoints | Tracks and albums, 40-item chunks, optimistic UI rollback |
| Lyrics | Documented LRCLIB `/api/get` | Synced LRC, plain fallback, instrumental/unavailable/error states |

librespot does **not** provide supported public APIs for Spotify Web API
library mutations, recommendations, Home metadata, artist catalog pages, or
lyrics. These remain separate integrations rather than private Spotify calls.

## Current Spotify constraints

- February 2026 removed artist top tracks and browse new releases for this
  access mode, and replaced content-specific save/check endpoints with generic
  URI library endpoints.
- Recommendations, related artists, audio features/analysis, and access to
  Spotify-owned algorithmic/editorial playlists remain unavailable to newer
  Development Mode apps. Rustify does not imitate or mislabel them.
- Development Mode requires the app owner to have Premium, allows five users,
  and exposes a reduced endpoint set.
- Refresh tokens expire after six months. Terminal `invalid_grant` now clears
  the rejected credential and emits logged-out state instead of looping.
- Developer quota failures may return 429 (including `QUOTA_EXCEEDED`); the
  app exposes `Retry-After` and does not retry long/non-idempotent requests.
- `/me.product` may be omitted for newer apps. Premium is enforced when the
  field exists; otherwise librespot's streaming handshake is authoritative.

Primary references:

- https://developer.spotify.com/documentation/web-api/references/changes/february-2026
- https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide
- https://developer.spotify.com/documentation/web-api/reference/save-library-items
- https://developer.spotify.com/documentation/web-api/reference/check-library-contains
- https://developer.spotify.com/documentation/web-api/reference/get-recently-played
- https://developer.spotify.com/documentation/web-api/reference/get-users-top-artists-and-tracks
- https://developer.spotify.com/blog/2024-11-27-changes-to-the-web-api
- https://developer.spotify.com/blog/2026-06-18-refresh-token-expiration
- https://developer.spotify.com/blog/2026-07-23-web-api-quota-updates
- https://lrclib.net/docs

## Root causes corrected

- Setup used `height: 100%` below a separate 46 px drag strip, centering 23 px
  too low and clipping small windows. A single flex column now gives Setup the
  exact remaining viewport and an accessible scroller.
- The native range retained a 2 px user-agent margin while the custom rail did
  not. Explicit margin/track/thumb geometry now shares the same centerline.
- Home threw away history context, deduplicated only tracks, and used the first
  six playlists as quick access. It now ranks context-aware activity with a
  stable library fallback and loads each shelf independently.
- Account click called `logout` directly. It now opens Profile; sign-out is an
  explicit action inside that view.
- Artist used `Promise.all` over a removed top-tracks endpoint and an invalid
  release limit of 50, so either failure emptied the page. Supported requests,
  a limit of 10, pagination, and partial states replace it.
- Save/check used deprecated content-specific endpoints and degraded 403 to
  false. Generic library URI endpoints now preserve the actual server state.
- Webview reload called restore even when Rust still owned a live session,
  producing competing refresh rotations during HMR. Both sides now reuse the
  active session.

## Verification record

- `cargo fmt --check`: pass.
- `cargo test`: 10 passed, covering recent grouping, relevance ordering and
  deduplication, release-edition deduplication, bitrate mapping, exact EQ
  bypass, invalid EQ rejection, legacy settings, URI formation, and LRC parsing.
- `cargo clippy --all-targets -- -D warnings`: pass.
- `npm run check`: zero errors/warnings.
- `npm run build`: pass.
- `npm run tauri build`: pass; optimized executable and NSIS installer built.
- Live Tauri: account-ranked Home data, Settings/device enumeration and EQ
  apply/restore, 760×520 layout, native fullscreen entry/F11 exit, PlayerBar
  suppression in fullscreen, available native controls, LRCLIB synchronized
  line controls, manual-follow recovery, rate-limit/partial states, account
  restore, and runtime log inspection.
- Track and album states were read, toggled, re-read, restored, and re-read;
  results were `false→true→false` and `true→false→true` respectively.
- Setup geometry: usable center and card center both 383 px at 1100×720;
  780×520 becomes a scrollable 474 px usable region with all content reachable.
- Volume geometry: input and rail center are both y=663 with zero input margin.

## Remaining external limitations

No Spotify-native lyrics, recommendations, related artists, popularity ranking,
Jams, Blend creation, queue reorder/removal, or free-tier librespot playback.
LRCLIB availability/accuracy is external. Offline metadata is unavailable.
Device transfer was deliberately not triggered during verification because a
different device was actively playing.

## See also

[[architecture]] · [[auth-and-tokens]] · [[known-limitations]] ·
[[external-dependencies]] · [[Home.svelte]] · [[ArtistView.svelte]] ·
[[NowPlaying.svelte]] · [[library.rs]] · [[mod.rs|lyrics/mod.rs]] · [[MOC]]
