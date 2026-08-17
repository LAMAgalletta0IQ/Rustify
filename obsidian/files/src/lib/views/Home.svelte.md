---
tags: [file, frontend, ui]
---
# `src/lib/views/Home.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **315 lines**

The largest frontend file. Library browsing across three sections, with
pagination and inline detail.

## Key items

### Constants
- `PAGE = 50` — playlists, albums, liked songs.
- `TRACK_PAGE = 100` — the Web API allows 100 for playlist tracks, not 50.

### State
| Name | Purpose |
| --- | --- |
| `section` | `"playlists" \| "albums" \| "liked"` |
| `playlists`, `albums`, `liked` | Cached per section |
| `loading`, `loadingMore` | Separate, so paging does not blank the list |
| `exhausted` | `Record<Section, boolean>` — hides "Load more" |
| `detail` | `{title, uri, tracks, playlistId, exhausted} \| null` |

### `likedUri` (`$derived`)
`api.likedSongsUri(store.auth.userId)`, or `null` if the id is missing. Gives
Liked Songs a real playable context.

### Functions
- **`load()`** — first page for the active section, **only if empty** (acts as
  a cache).
- **`loadMore()`** — appends the next page; sets `exhausted` when a short page
  returns.
- **`loadMoreTracks()`** — the same for tracks inside an open playlist.
- **`openPlaylist(p)` / `openAlbum(a)`** — build `detail`. Albums set
  `playlistId: null` and `exhausted: true`, since `getAlbumTracks` returns
  everything at once.

### The effect
```svelte
$effect(() => {
  section;
  untrack(load);
});
```

## Inputs / outputs / side effects

Reads `store.auth.userId`. Calls five library commands. Play buttons issue
`loadContext`.

## Dependencies

**Imports:** `svelte` (`untrack`), [[api.ts]], [[store.svelte.ts]],
[[TrackList.svelte]], [[types.ts]]
**Imported by:** [[App.svelte]]

## Notable logic / gotchas

> ### Why `untrack`
> `load()` reads `playlists.length` **synchronously** before its first `await`,
> so that read is tracked. Without `untrack`, appending a page would re-trigger
> the effect. It converged (the `.length === 0` guard prevented a refetch) but
> re-ran needlessly on every page. See [[frontend-svelte]].

- **`exhausted` is inferred from a short page**, not a total count — the
  simplest correct signal, and it works even when Spotify omits `total`.
- **Section caches are never invalidated.** Saving a track from
  [[TrackList.svelte]] does not refresh the Liked Songs list; switching tabs
  shows stale data until relaunch.
- **`detail.uri` is what makes playback continue.** An earlier version stored
  only `{title, tracks}` and played `tracks[0].uri`, so a playlist played one
  song and stopped. Passing `contextUri` to [[TrackList.svelte]] is the fix.
- **Detail rendering here is inline**, unlike [[Search.svelte]], which
  delegates to [[AlbumView.svelte]]. Two patterns for one job — see
  [[frontend-views]].
- `detail!` non-null assertions inside the template are safe within the
  `{:else if detail}` branch.

## See also

[[TrackList.svelte]] · [[api.ts]] · [[library.rs]] · [[AlbumView.svelte]] ·
[[playback-and-connect]] · [[frontend-views]] · [[MOC]]
