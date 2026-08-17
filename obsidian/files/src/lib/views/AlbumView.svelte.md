---
tags: [file, frontend, ui]
---
# `src/lib/views/AlbumView.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **109 lines**

## Purpose

Album detail: header with artwork, artists, a Play button and a save toggle,
plus the track list.

## Key items

### Props
`album: AlbumSummary`, `onBack: () => void`.

### State
`tracks`, `loading`, `saved: boolean | null`.

> `saved` starts `null` — **unknown**, not "not saved". There is no
> `/me/albums/contains` call here, so the heart reflects only actions taken in
> this session. Reopening the view resets it to unknown.

### The effect
Loads `api.getAlbumTracks(album.id)` when `album.id` changes, with a
`cancelled` flag set in the cleanup so a slow response for a previously viewed
album cannot overwrite the current one.

### `toggleSaved()`
Optimistic: flips `saved`, calls `api.setAlbumsSaved([album.id], next)`, and
**reverts on failure** while writing the message to `store.error`.

### Render
Header (art, name, artists, Play, ♡/♥) then
`<TrackList {tracks} contextUri={album.uri} />`.

## Inputs / outputs / side effects

Calls `get_album_tracks` and `set_albums_saved` (**mutates the user's
library**). Play issues `loadContext(album.uri)`.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[TrackList.svelte]],
[[types.ts]]
**Imported by:** [[Search.svelte]], and indirectly via [[ArtistView.svelte]]

## Notable logic / gotchas

> **Album track rows have no cover art.** `/albums/{id}/tracks` returns
> *simplified* track objects with no nested album, so every row's `imageUrl` is
> `null`. A source comment notes this and the header carries the artwork
> instead. See [[library.rs]].

- **`contextUri` is passed**, so clicking track 5 plays the album from track 5
  onward rather than that track alone.
- **The `cancelled` flag matters** when navigating album → album quickly from
  an artist page; without it, an earlier slow response could clobber newer
  tracks.
- **The optimistic save has no confirmation read.** If the request fails the
  heart reverts, but a success is never verified against the server.
- No pagination — `getAlbumTracks` fetches a single page of 50, which covers
  essentially all albums.

## See also

[[ArtistView.svelte]] · [[Search.svelte]] · [[TrackList.svelte]] ·
[[library.rs]] · [[known-limitations]] · [[frontend-views]] · [[MOC]]
