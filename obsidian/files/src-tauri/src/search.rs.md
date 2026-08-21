---
tags: [file, backend, webapi, rust]
---
# `src-tauri/src/search.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~239 lines**

> Until 2026-08 this note said search was not paginated. It now is —
> `search()` takes an `offset` and `SearchResults` carries `has_more`. See
> the corrected paragraph below.

## Purpose

Multi-type search across tracks, albums, artists and playlists in a single Web
API request.

## Key items

### Public types
- `SearchResults` — `tracks`, `albums`, `artists`, `playlists`, `has_more`.
  Derives `Default`, so an empty query returns empty vectors rather than an
  error. `has_more` is `true` if **any** category's `Wrap<T>` carried a
  `next` cursor in Spotify's response — [[Search.svelte]]'s "Show more
  results" button reads this and calls `search()` again with an incremented
  `offset`.
- `ArtistSummary` — `uri`, `id`, `name`, `image_url`.
- `PlaylistHit` — `uri`, `id`, `name`, `owner`, `image_url`.

`TrackSummary` and `AlbumSummary` are reused from [[library.rs]] rather than
redefined, so [[TrackList.svelte]] renders search results unchanged.

### Private wire types
`SearchResponse`, `Wrap<T>`, `WireImage`, `WireNamed`, `WireOwner`,
`WireAlbum`, `WireTrack`, `WireArtist`, `WirePlaylist`.

> **`Wrap<T>` holds `items: Vec<Option<T>>`, not `Vec<T>`.** Spotify returns
> `null` entries inside `items` for unavailable results. Without the `Option`,
> deserialisation fails outright on perfectly ordinary searches. The `null`s
> are dropped with `.flatten()`.

### `async fn search(api, token, query, limit, offset) -> SearchResults`
- Returns `SearchResults::default()` immediately for a blank query, avoiding a
  request that would 400.
- `type=track,album,artist,playlist` in one call, with `offset` forwarded so
  callers can page.
- `limit` clamped to `1..=MAX_SEARCH_LIMIT` (**10**), and 10 is also the default
  used by `search_spotify`.
- `has_more` is computed by checking whether **any** of the four wrapped
  response categories carried a `next` cursor.

> ### `/search` caps `limit` at 10
> Far below the 50 most endpoints accept, and exceeding it is a hard
> `400 Bad Request: Invalid limit` — not a silent truncation. The code clamped
> to 50 and defaulted to 20, so **every search failed** until this was found.
> The constant is shared with [[commands.rs]] so the clamp and the default
> cannot drift apart. The 50s in [[library.rs]] are correct for those endpoints.
- Each category is `Option` in the response, handled with
  `.map(...).unwrap_or_default()`.

### `fn pick_image(images)`
Closest to 300px, matching [[library.rs]] and [[player.rs]].

## Inputs / outputs / side effects

Network only, read-only. One `GET /v1/search` per call.

## Dependencies

**Imports:** `serde`, [[library.rs]] (`AlbumSummary`, `TrackSummary`),
[[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]]

## Notable logic / gotchas

- **The `Vec<Option<T>>` shape is not defensive over-engineering** — it is
  required by the API's real behaviour and is commented as such in the source.
- **Search results carry no container context.** A track from search has no
  playlist or album to continue into, which is why [[Search.svelte]] renders
  [[TrackList.svelte]] *without* `contextUri`, falling back to `load_tracks`.
  See [[playback-and-connect]].
- **Paginated per-category, but exposed as one `has_more` flag.** If
  tracks are exhausted but albums still have more, `has_more` is still
  `true` and the next page re-requests all four categories at the new
  offset — there is no independent "load more albums only". Acceptable
  because [[Search.svelte]]'s UI has no per-section pagination control
  either.
- The nested `.map(...)` blocks per category are repetitive; the shapes differ
  just enough that a shared helper would need generics over four unrelated
  target types.

## See also

[[library.rs]] · [[webapi.rs]] · [[commands.rs]] · [[Search.svelte]] ·
[[TrackList.svelte]] · [[known-limitations]] · [[backend-rust]] · [[MOC]]
