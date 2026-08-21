---
tags: [file, backend, webapi, library]
---
# `src-tauri/src/library.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~1014 lines**

> Until 2026-08 this was documented as a short, mostly-static adapter. It has
> grown substantially: a session-local saved-tracks cache (the artist-page
> N+1 fix), and followed-artist release aggregation with edition
> deduplication (backing [[Releases.svelte]]) are both new. This note now
> covers both.

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
**No longer the primary path** — [[commands.rs]]'s `get_artist_overview`
prefers `queryArtistOverview`'s real top-tracks field via
[[spotify/artist_extras.rs]] (Pathfinder), and only falls back to
`artist_tracks` when that field comes back empty (Pathfinder unreachable, or
the private field renamed under us). `artist_albums` clamps limit to 10 and
returns `AlbumPage { items, has_more }`.

### `saved_tracks_snapshot` / `liked_tracks_by_artist` / `invalidate_saved_tracks_cache`
Session-local cache fixing an N+1 problem: opening several artist pages in a
row used to re-walk the whole saved-tracks library (up to 500 tracks, in
pages of 50) once per page just to compute "which of this artist's tracks
does the user have liked". `saved_tracks_snapshot` now serves that walk from
a cached `SavedTracksSnapshot` when younger than `SAVED_TRACKS_CACHE_TTL`
(5 minutes) — a cold/expired cache costs the same bounded walk as before, a
warm one costs nothing. `liked_tracks_by_artist` just filters the snapshot by
`artist_ids`. The walk itself is capped at 500 tracks (ten pages of 50) so an
enormous library can't monopolize the API quota; the UI treats the result as
"the checked portion" when that cap is hit. `invalidate_saved_tracks_cache`
is called after any save/unsave so the next artist page reflects it
immediately rather than waiting out the TTL — see [[state.rs]]'s
`saved_tracks_cache` field, which owns the `RwLock` this all locks.

### `followed_releases` / `deduplicate_releases` / `edition_key`
Backs [[Releases.svelte]]. Spotify has no aggregate "new releases from
artists I follow" endpoint, so this paginates by followed-artist cursor
(`followed_artists(..., 8, after)`) and, for each of those 8 artists in that
page, sequentially fetches their 10 most recent albums — deliberately
sequential rather than concurrent, to avoid an eight-request burst against
the per-client rolling quota; a page is small enough that this stays
responsive, and results from any one artist that fails still make it into
`partial_errors` rather than failing the whole page (except
`RateLimited`/`SessionExpired`, which abort immediately since retrying
wouldn't help). `deduplicate_releases` then collapses different pressings of
the same release — a deluxe edition, remaster, or the same date advertised as
both a single and part of an album — using `edition_key`: lowercased primary
artist + title (with "(deluxe edition)"/"(remastered)"/etc. suffixes
stripped) + album type + release year.

Tests cover generic URI formation and the album-context/loose-track grouping
and ordering rule.

## Dependencies

**Imports:** [[webapi.rs]], `crate::search::ArtistSummary`, `tokio::sync::RwLock`
**Imported by:** [[commands.rs]], [[state.rs]] (`SavedTracksSnapshot`,
`saved_tracks_cache`)

## See also

[[webapi.rs]] · [[commands.rs]] · [[state.rs]] · [[Home.svelte]] ·
[[ArtistView.svelte]] · [[Releases.svelte]] · [[spotify/artist_extras.rs]] ·
[[2026-08-capability-audit]] · [[MOC]]
