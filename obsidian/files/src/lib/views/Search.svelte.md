---
tags: [file, frontend, ui]
---
# `src/lib/views/Search.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **157 lines**

## Purpose

Debounced multi-type search, and the navigation host for album and artist
drill-downs.

## Key items

### State
| Name | Purpose |
| --- | --- |
| `query` | Bound to the input |
| `results` | `SearchResults \| null` |
| `loading` | Spinner flag |
| `timer` | Debounce handle |
| `openAlbum` | `AlbumSummary \| null` — drill-down |
| `openArtist` | `ArtistSummary \| null` — drill-down |

### `onInput()` / `run()`
`onInput` clears any pending timer and schedules `run()` **300 ms** later, so
typing does not fire a request per keystroke. `run()` trims the query, clears
results when blank, and calls `api.searchSpotify(q)`.

### Render structure
```svelte
{#if openAlbum}      → AlbumView
{:else if openArtist} → ArtistView  (onOpenAlbum re-parents)
{:else}               → input + four result sections
{/if}
```

Results render Tracks (via [[TrackList.svelte]]), then Albums, Playlists and
Artists as card grids. Each section only renders if non-empty.

## Inputs / outputs / side effects

Calls `search_spotify`. Playlist cards call `loadContext` directly — playlists
have no detail view. Album and artist cards navigate instead.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[TrackList.svelte]],
[[AlbumView.svelte]], [[ArtistView.svelte]], [[types.ts]]
**Imported by:** [[App.svelte]]

## Notable logic / gotchas

> **`TrackList` is rendered without `contextUri` here.** Search results have no
> container, so clicking a track uses `load_tracks` over the whole result list.
> Passing a context would be wrong; omitting it is deliberate. See
> [[playback-and-connect]].

- **Drill-downs preserve the results behind them.** `query` and `results` are
  untouched while an album or artist is open, so "Back" is instant with no
  refetch.
- **Artist → album re-parents** by clearing `openArtist` and setting
  `openAlbum` in the same handler, so "Back" from the album returns to the
  *search results*, not the artist. A minor UX wrinkle rather than a bug.
- **The debounce is not cancelled on unmount.** A pending timer can fire after
  navigation; harmless, since it only assigns local state on a destroyed
  component.
- **Not paginated** — one page per type. See [[known-limitations]].
- Album/artist/playlist cards use `{#each}` without an `{:else}`; empty
  categories are excluded by the enclosing `{#if}`.

## See also

[[AlbumView.svelte]] · [[ArtistView.svelte]] · [[TrackList.svelte]] ·
[[search.rs]] · [[api.ts]] · [[frontend-views]] · [[MOC]]
