---
tags: [file, frontend, ui]
---
# `src/lib/views/ArtistView.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **142 lines**

## Purpose

Artist detail: avatar and name, popular tracks, and a grid of albums and
singles.

## Key items

### Props
`artist: ArtistSummary`, `onBack: () => void`,
`onOpenAlbum: (a: AlbumSummary) => void`.

`onOpenAlbum` delegates navigation upward to [[Search.svelte]] rather than
nesting an album view inside this one — keeping all drill-down state in a
single owner.

### State
`top: TrackSummary[]`, `albums: AlbumSummary[]`, `loading`.

### The effect
Fetches both lists **concurrently**:
```ts
const [t, a] = await Promise.all([
  api.getArtistTopTracks(id),
  api.getArtistAlbums(id),
]);
```
Guarded by the same `cancelled` cleanup flag used in [[AlbumView.svelte]].

### Render
Header (round avatar, name, Play → `loadContext(artist.uri)`), then "Popular"
via [[TrackList.svelte]], then an "Albums & singles" grid.

## Inputs / outputs / side effects

Calls `get_artist_top_tracks` and `get_artist_albums`. Play issues
`loadContext`. Album cards invoke `onOpenAlbum`.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[TrackList.svelte]],
[[types.ts]]
**Imported by:** [[Search.svelte]]

## Notable logic / gotchas

> **`TrackList` is rendered *without* `contextUri`.** Top tracks are a
> synthesised list, not a browsable Spotify context, so clicking one uses
> `load_tracks` over the ten results. A source comment states this. See
> [[known-limitations]].

- **The header Play button *does* use `loadContext(artist.uri)`** — an artist
  URI is a valid context even though its top-tracks list is not. The two
  controls therefore take different paths deliberately.
- **`Promise.all` halves perceived load time** versus sequential awaits.
- `getArtistAlbums` requests `include_groups=album,single` in [[library.rs]],
  so compilations and appearances are excluded.
- Album covers use `loading="lazy"`, meaningful for prolific artists.

## See also

[[AlbumView.svelte]] · [[Search.svelte]] · [[TrackList.svelte]] ·
[[library.rs]] · [[known-limitations]] · [[frontend-views]] · [[MOC]]
