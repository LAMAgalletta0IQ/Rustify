---
tags: [file, backend, pathfinder, rust]
---
# `src-tauri/src/spotify/concerts.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **169 lines**

Parses upcoming tour dates ("Artist On Tour" cards) from the same
`queryArtistOverview` Pathfinder response [[artist_extras.rs]] also reads —
`data.artistUnion.goods.concerts`.

## Key items

### `fn parse(data) -> AppResult<ConcertFeed>`
Errors only if `artistUnion` itself is absent (a real fetch failure);
`goods.concerts` specifically being absent is a supported empty feed, not an
error — the two are deliberately distinguished.

### `ConcertFeed::unavailable()`
Constructs the "never actually fetched" state (added 2026-08, for when the
whole `queryArtistOverview` call fails outright and `get_artist_overview`
falls back to REST-only) — distinct from a successful fetch that just found
zero events.

## Dependencies

**Imports:** `serde_json` — no crate-internal imports beyond `error.rs`
**Imported by:** [[mod.rs|spotify/mod.rs]] (folded into `artist_overview`),
[[artist_extras.rs]] (shares the same response)

## See also

[[pathfinder.rs|spotify/pathfinder.rs]] · [[artist_extras.rs]] ·
[[ArtistView.svelte]] · [[MOC]]
