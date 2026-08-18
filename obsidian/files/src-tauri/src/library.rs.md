---
tags: [file, backend, webapi, library]
---
# `src-tauri/src/library.rs`

Typed Spotify metadata/library adapter. It flattens Web API wire envelopes into
playlist, album, artist, track, page, and recent-activity IPC models.

## Current endpoint contracts

- Reads: `/me/playlists`, `/playlists/{id}/items`, `/me/tracks`, `/me/albums`,
  `/albums/{id}/tracks`, `/me/following`, `/me/player/recently-played`,
  `/me/top/{tracks|artists}`, `/artists/{id}`, `/artists/{id}/albums`.
- Saves/removes: generic `PUT`/`DELETE /me/library?uris=...`.
- Saved checks: `GET /me/library/contains?uris=...`.
- Generic library operations chunk at Spotify's current 40-URI maximum.

`collapse_history` consumes newest-first history. It selects the real playback
context when it is an album, playlist, or artist, retains `played_at` and the
latest starting track, and deduplicates by context URI. Null/unsupported
contexts become individual tracks and deduplicate by track URI. Server history
provides persistence.

`artist_tracks` is a truthful fallback after Spotify removed artist top tracks:
it samples the first ten distinct tracks from recent artist releases and
injects the release name/art missing from simplified album-track objects.
`artist_albums` clamps limit to 10 and returns `AlbumPage { items, has_more }`.

Tests cover generic URI formation and the album-context/loose-track grouping
and ordering rule.

## See also

[[webapi.rs]] · [[commands.rs]] · [[Home.svelte]] · [[ArtistView.svelte]] ·
[[2026-08-capability-audit]] · [[MOC]]
