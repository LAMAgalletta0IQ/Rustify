---
tags: [file, frontend, ui]
---
# `src/lib/components/TrackList.svelte`

**Module:** [[frontend-components]] · **Language:** Svelte 5 · **184 lines**

The most reused component — every view that shows tracks renders it.

## Key items

### Props
```ts
tracks: TrackSummary[]
contextUri?: string | null   // default null
```

### `playTrack(t)` — the central decision

| `contextUri` | Call | Effect |
| --- | --- | --- |
| Provided | `loadContext(contextUri, t.uri)` | Plays the container from this track; playback continues |
| `null` | `loadTracks(tracks.map(uri), t.uri)` | Ad-hoc list of the visible rows |

> Getting this wrong is why a playlist once played one song and stopped. See
> [[playback-and-connect]].

### Saved (♥) state
`saved: Record<string, boolean>` — absent means *not yet known*.

An `$effect` looks up the visible rows whenever `tracks` changes, **chunking by
50** because `/me/tracks/contains` caps there ([[library.rs]]). A `cancelled`
flag in the cleanup prevents a stale response from applying. Failures are
swallowed deliberately — a comment notes that leaving hearts unknown is not
worth an error banner.

### `toggleSaved(t)`
Optimistic flip, `api.setTracksSaved([t.id], next)`, revert plus
`store.error` on failure.

### Row layout
A five-column grid — index, art, name/artist, album, duration — plus ♡ and ＋
buttons that fade in on hover (`opacity: 0` → `1`), with saved hearts always
visible via `.heart.on`.

`playing` (`$derived`) accents the row matching `store.playback.track?.uri`.

## Inputs / outputs / side effects

Calls `get_tracks_saved`, `set_tracks_saved` (**mutates the library**),
`add_to_queue`, and one of the two load commands.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[types.ts]]
**Imported by:** [[Home.svelte]], [[Search.svelte]], [[AlbumView.svelte]],
[[ArtistView.svelte]]

## Notable logic / gotchas

- **The saved-state effect writes `saved` after an `await`**, so that read is
  untracked and cannot loop. Fragile but correct — see [[frontend-svelte]].
- **`getTracksSaved` returns positionally**; the code zips `chunk[n]` to
  `flags[n]`. A backend change to that ordering would silently mislabel hearts.
- **Rows are keyed `t.uri + i`, not `t.uri`.** Playlists can legitimately
  contain the same track twice, which would make a URI-only key non-unique.
- **`t.id` can be empty** for local tracks; `toggleSaved` returns early and the
  effect filters them out with `.filter(Boolean)`.
- **Album rows have no artwork** because [[library.rs]] cannot supply it from
  `/albums/{id}/tracks`; the `.ph` placeholder renders instead.
- The lookup runs per mount, so navigating away and back re-queries.

## See also

[[playback-and-connect]] · [[library.rs]] · [[api.ts]] ·
[[frontend-components]] · [[Home.svelte]] · [[AlbumView.svelte]] · [[MOC]]
