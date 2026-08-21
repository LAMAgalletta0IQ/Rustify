---
tags: [file, backend, lyrics, integration]
---
# `src-tauri/src/lyrics/mod.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~518 lines**

> **Correction (2026-08):** this note previously claimed the lyrics client
> "never scrapes Spotify... or calls a private endpoint" and was LRCLIB-only.
> That stopped being true at some point before this review — `fetch` now
> tries **Spotify's own first-party color-lyrics endpoint first**, through
> the authenticated `session.spclient()`, and only falls back to LRCLIB when
> Spotify has nothing for the track. This is the source of the synced-lyrics
> color theming, language/RTL detection, and provider name (e.g.
> "Musixmatch") that [[NowPlaying.svelte]] renders — none of which LRCLIB
> provides. The rest of this note is corrected to match.

## Purpose

Lyrics lookup with two providers, tried in order: Spotify's own private
color-lyrics endpoint (richer — synced timing, theming colors, language,
provider attribution), falling back to the public [LRCLIB](https://lrclib.net)
API when Spotify has nothing for the track or the request fails. Read-only
either way — no lyrics are ever written back anywhere.

## Key items

### `struct LyricsResult`
The unified shape both providers map onto: `provider` (e.g. `"Spotify"` or
`"LRCLIB"`), `status` (`"available"` | `"instrumental"` | `"unavailable"`),
`sync_type` (`"lineSynced"` | `"unsynced"` | absent), `language`, `is_rtl`,
`colors: Option<LyricsColors>`, `plain: Option<String>`,
`synced: Vec<LyricsLine>`.

### `struct LyricsColors`
`background`, `text`, `highlight_text` — **signed ARGB** integers exactly as
Spotify's own lyrics surface uses them. [[NowPlaying.svelte]]'s `rgbFromArgb`
converts these to CSS hex for the themed lyrics background.

### `async fn fetch(session, track_uri, track_name, artist_name, album_name, duration_ms) -> AppResult<LyricsResult>`
1. Validates the URI is a canonical track URI (`validate_track_uri`).
2. Tries `fetch_spotify` first. A successful, non-`"unavailable"` result
   returns immediately.
3. Any failure or an explicit "unavailable" result logs at debug (never a
   user-facing error — a provider miss is normal) and falls through to
   `fetch_lrclib`.

### `async fn fetch_spotify(session, track_uri, duration_ms) -> AppResult<Option<LyricsResult>>`
Calls `session.spclient().get_lyrics(&id)` — an authenticated request
through librespot's own SpClient client, not a scrape or an unofficial HTTP
call. Caps the response at `MAX_LYRICS_BYTES` (1 MiB) before parsing.
`SpotifyLyricsResponse`'s fields are deliberately **local and
forward-compatible**: librespot's own public metadata model uses an enum for
`syncType`, so a newly introduced Spotify value there would reject the whole
otherwise-usable response — this module reads `sync_type` as a plain
`String` instead and classifies it itself in `from_spotify`.

### `fn from_spotify(response, duration_ms) -> LyricsResult`
Maps Spotify's line-synced or plain response onto `LyricsResult`, including
`colors`, `language`, `is_rtl_language`, and `provider_display_name` (the
real attribution string Spotify supplies, e.g. "Musixmatch" — not a generic
"Spotify" label).

### `async fn fetch_lrclib(track_name, artist_name, album_name, duration_ms) -> AppResult<LyricsResult>`
The original client, unchanged: `GET https://lrclib.net/api/get` with exact
track, first artist, album, and rounded duration parameters, a 12-second
timeout, and an identifying Rustify repository User-Agent. Never sends
credentials — LRCLIB is a public, unauthenticated API. A 404 is a normal
`"unavailable"` result, a 429 becomes typed `RateLimited` with
`Retry-After`, and provider 5xx becomes `ServiceUnavailable`. Always reports
`provider: "LRCLIB"`, `language: None`, `is_rtl: false`, `colors: None` —
none of which LRCLIB's response carries.

### `parse_lrc`
Supports multiple timestamps on one line, ignores metadata tags, converts
timestamps to milliseconds, preserves blank timing rows, and sorts. Unit
test covers multiple timestamps and out-of-order input. Only used for the
LRCLIB path — Spotify's own response is already structured per-line.

## Session-level caching (in [[state.rs]], not this file)
`AppState.lyrics_cache: RwLock<HashMap<String, LyricsResult>>` avoids
re-fetching for a URI the frontend already has — playback events replace
frontend snapshots but must not trigger another lyrics request for the same
track. `AppState.lyrics_requests: Mutex<HashMap<String, Arc<Mutex<()>>>>` is
a per-track gate preventing [[NowPlaying.svelte]] and a concurrent refresh
from both firing a request for the same URI at once. Both are cleared on
logout.

## Inputs / outputs / side effects

- **Network:** Spotify's private color-lyrics endpoint via the authenticated
  librespot session (no separate HTTP client — reuses the streaming
  connection); a public, unauthenticated `GET` to `lrclib.net` as fallback.
- Pure otherwise — no filesystem, no writes.

## Dependencies

**Imports:** `serde`, `librespot::core::{session::Session, SpotifyUri}`,
[[error.rs]]
**Imported by:** [[commands.rs]] (`get_lyrics`), [[state.rs]] (`lyrics_cache`,
`lyrics_requests`)

## Notable logic / gotchas

- **Spotify-first, not LRCLIB-only.** See the correction at the top of this
  note — the two providers are tried in a fixed order and the result shape
  doesn't tell the caller which one actually answered beyond the `provider`
  string.
- **A Spotify failure is swallowed, not surfaced.** `fetch_spotify` errors
  (network, parse, oversized response) are logged at debug and treated the
  same as "Spotify has nothing for this track" — the user only ever sees a
  real error if LRCLIB *also* fails.
- **`SpotifyLyricsBody`'s stringly-typed fields are a deliberate
  compatibility choice**, not sloppiness — see the forward-compatibility
  note under `fetch_spotify` above.
- **This is a private, unofficial Spotify API call**, same category as
  Pathfinder/SpClient elsewhere in the app (see [[jams/spclient.rs]],
  [[spotify/pathfinder.rs]]) — it can change shape without notice, which is
  exactly why the response fields are read defensively.

## See also

[[NowPlaying.svelte]] · [[state.rs]] · [[commands.rs]] ·
[[external-dependencies]] · [[2026-08-capability-audit]] · [[backend-rust]] ·
[[MOC]]
