---
tags: [file, backend, webapi, rust]
---
# `src-tauri/src/search.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **202 lines**

## Purpose

Multi-type search across tracks, albums, artists and playlists in a single Web
API request.

## Key items

### Public types
- `SearchResults` — `tracks`, `albums`, `artists`, `playlists`. Derives
  `Default`, so an empty query returns empty vectors rather than an error.
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

### `async fn search(api, token, query, limit) -> SearchResults`
- Returns `SearchResults::default()` immediately for a blank query, avoiding a
  request that would 400.
- `type=track,album,artist,playlist` in one call.
- `limit` clamped to `1..=50`.
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
- **Not paginated.** One page per query (default 20 per type). Listed in
  [[known-limitations]].
- The nested `.map(...)` blocks per category are repetitive; the shapes differ
  just enough that a shared helper would need generics over four unrelated
  target types.

## See also

[[library.rs]] · [[webapi.rs]] · [[commands.rs]] · [[Search.svelte]] ·
[[TrackList.svelte]] · [[known-limitations]] · [[backend-rust]] · [[MOC]]
