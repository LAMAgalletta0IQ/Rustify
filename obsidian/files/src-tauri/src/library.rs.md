---
tags: [file, backend, webapi, rust]
---
# `src-tauri/src/library.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **351 lines**

The largest Web API module. Playlists, saved albums and tracks, save/unsave,
and artist data.

## Public types (sent to the UI)

Flat, UI-shaped structs — deliberately not Spotify's nested envelopes:

| Type | Fields |
| --- | --- |
| `PlaylistSummary` | `uri`, `id`, `name`, `owner`, `image_url`, `track_count` |
| `AlbumSummary` | `uri`, `id`, `name`, `artists`, `image_url` |
| `TrackSummary` | `uri`, `id`, `name`, `artists`, `album`, `image_url`, `duration_ms` |

## Private wire types

`Page<T>`, `WirePlaylist`, `WireTrackRef`, `WireNamed`, `WireImage`,
`WireArtist`, `WireAlbum`, `WireTrack`, `SavedAlbum`, `SavedTrack`,
`PlaylistItem` — mirroring Spotify's JSON, converted via `From` impls.

## Functions

### Reading
| Function | Endpoint | Notes |
| --- | --- | --- |
| `playlists` | `/me/playlists` | Limit capped at 50 |
| `playlist_tracks` | `/playlists/{id}/tracks` | Limit capped at **100** |
| `saved_tracks` | `/me/tracks` | Liked Songs |
| `saved_albums` | `/me/albums` | |
| `album_tracks` | `/albums/{id}/tracks` | Single page of 50 |
| `artist_top_tracks` | `/artists/{id}/top-tracks` | Unwraps `{tracks: [...]}` |
| `artist_albums` | `/artists/{id}/albums` | `include_groups=album,single` |

### Writing
| Function | Endpoint | Notes |
| --- | --- | --- |
| `set_tracks_saved` | PUT/DELETE `/me/tracks` | Body `{"ids": [...]}` |
| `set_albums_saved` | PUT/DELETE `/me/albums` | Same shape |
| `tracks_saved` | GET `/me/tracks/contains` | Returns one bool per id, **in order** |

Both setters return `Ok(())` early on an empty id list rather than issuing a
pointless request.

### `fn pick_image(images) -> Option<String>`
Selects the image closest to 300px — same reasoning as [[player.rs]]'s
`pick_cover`: avoid decoding oversized JPEGs on weak hardware.

## Inputs / outputs / side effects

Network only. `set_*_saved` **mutates the user's Spotify library** — the only
write operations in the app besides queue append.

## Dependencies

**Imports:** `serde`, `serde_json`, [[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]]; `TrackSummary` and `AlbumSummary` are re-used
by [[search.rs]] and [[queue.rs]]

## Notable logic / gotchas

- **`PlaylistItem.track` carries `#[serde(alias = "item")]`, and playlists are
  empty without it.** `/playlists/{id}/items` keys its rows `item`, not
  `track`. Verified live 2026-08-17: `fields=items(track(name))` comes back
  `{"items":[{}]}` while `fields=items(item(name))` returns the track.

  > This is the worst-behaved failure in the file. `track` is `Option`, so the
  > wrong key is not a deserialisation error — every row simply parsed to
  > `None`, `filter_map` discarded all of them, and the playlist opened to
  > "Nothing here." **Nothing was logged**, at any level: [[webapi.rs]] only
  > reports failed *requests*, and this request succeeded with a 200. The two
  > gotchas below interact to hide it — the `Option` exists for legitimately
  > null tracks, and it silently absorbs a renamed key too.

- **Playlist items can have a `null` track** (local files, unavailable
  content). `playlist_tracks` uses `filter_map(|i| i.track)` — without it,
  deserialisation would fail on ordinary playlists.
- **`album_tracks` rows have no cover art.** `/albums/{id}/tracks` returns
  *simplified* track objects with no nested album, so `image_url` is always
  `None`. [[AlbumView.svelte]] compensates with a header image. Documented in
  [[known-limitations]].
- **`tracks_saved` returns positionally** — bool *i* corresponds to id *i*.
  [[TrackList.svelte]] relies on this when zipping results back to rows.
- **The 50-id cap** on `/me/tracks/contains` is not enforced here; the caller
  chunks. Worth remembering when adding a new caller.
- **`WireNamed` uses `#[serde(alias = "name")]` on `display_name`** so the same
  struct deserialises both playlist owners (`display_name`) and artists
  (`name`).
- Limits are clamped with `.min(50)` / `.min(100)` so a caller cannot exceed
  Spotify's maximum and get a 400.

## See also

[[webapi.rs]] · [[commands.rs]] · [[search.rs]] · [[queue.rs]] ·
[[Home.svelte]] · [[TrackList.svelte]] · [[AlbumView.svelte]] ·
[[external-dependencies]] · [[backend-rust]] · [[MOC]]
