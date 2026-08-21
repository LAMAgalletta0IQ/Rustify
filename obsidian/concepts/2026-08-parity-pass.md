---
tags: [concept, audit, spotify, audio, ui]
---
# 2026-08 parity implementation pass

Implemented against pinned librespot 0.8.0, rodio 0.21.1, CPAL 0.16.0,
Tauri 2.11.5, Svelte 5.56.9, and Spotify's February 2026 Development Mode API.

## Data and navigation

- `relevance.rs` ranks Home quick access from the real 50-play history window
  plus account-scoped persisted opens/plays. It decays local recency,
  incorporates API frequency/order, deduplicates URI, caps storage at 100, and
  isolates each Spotify user. No arbitrary playlist-prefix fallback remains.
- Followed releases use `/me/following?type=artist` plus artist albums. Pages
  cover eight followed artists, return the next cursor, surface partial
  failures, sort actual dates, and deduplicate market/deluxe/remaster editions
  by artist, normalized title, type, and release year while preserving truly
  distinct years.
- Releases filters type/date, loads incrementally, checks save state, and uses
  correct album contexts. “For you” uses real top tracks/artists and explicitly
  identifies Rustify—not Spotify—as the arranger.
- Search has labeled types, a top result, empty/error/loading states, explicit
  metadata, native focus, and offset loading in 10-result API pages.
- Artist pages have real follow state/actions, adaptive imagery, playable
  release-derived tracks, show-more, liked tracks matched by artist ID,
  discography filters/date/type, saving, queueing, and pagination. Unavailable
  monthly listeners, verification, picks, and related artists are not invented.

## Player and lyrics

Now Playing remains mounted while Tauri `setFullscreen` changes the window, so
mode changes cannot reload a track. F11 toggles and Escape exits fullscreen;
native window actions remain available. The responsive composition balances
artwork/timeline/metadata with lyrics or queue. The artwork overlay is
keyboard-focusable and contains only connected playback, volume, view,
fullscreen, and close controls.

LRCLIB line timestamps drive normal/fullscreen illumination and click-to-seek.
Past/current/upcoming lines differ visibly. Manual wheel/touch scrolling
suspends following and exposes Resume following. Plain lyrics never receive
guessed timestamps; word highlighting is absent because LRCLIB LRC supplies
line timestamps only.

Central motion tokens live in `app.css`. The OS reduced-motion preference and
Rustify preference stop nonessential motion without changing focus semantics.

## Audio and persistence

See [[playback-and-connect]] for the sink boundary. `audio.rs` owns real quality
selection, device status/fallback, EQ validation, presets, smoothing, automatic
headroom, and sample processing. New settings use serde defaults, so old
`settings.json` files migrate without a separate schema step. Custom presets
support create/select/rename/delete/reset and persistence; the curve plots the
exact active band gains.

Lossless and friend activity are explicit verified limitations, not mock UI.

## Troubleshooting

- Refresh outputs after device connection/rename. A missing saved name falls
  back to System default and the status reports why.
- Sign out/in once to grant the newly requested `user-follow-modify` scope.
- A release warning means one or more artist catalogs failed; rendered items
  remain real, but the page does not claim completeness.
- Spotify 429 `Retry-After` remains visible; aggregation does not silently hide
  rate-limited artists.

Focused tests cover bitrate mapping, exact EQ bypass, invalid EQ state, legacy
settings, recent grouping/frequency, release deduplication, relevance ordering
and deduplication, generic library URIs, and LRC parsing.

## See also

[[2026-08-capability-audit]] · [[known-limitations]] ·
[[playback-and-connect]] · [[MOC]]
