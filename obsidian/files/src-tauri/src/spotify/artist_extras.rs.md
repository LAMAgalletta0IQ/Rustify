---
tags: [file, backend, pathfinder, rust]
---
# `src-tauri/src/spotify/artist_extras.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **165 lines** · added 2026-08

Parses the parts of `queryArtistOverview`'s response that [[concerts.rs]]
doesn't: listener stats and top tracks. Both live in the exact same
`data.artistUnion` payload `artist_overview` already fetches for concerts,
so folding them in here costs nothing extra over the wire — replaces what
used to be a REST fan-out (fetch five recent albums, then every track of
each) with fields sitting in a response already being made.

## Key items

### `fn parse_stats(data) -> ArtistStats`
`monthly_listeners`/`followers` from `artistUnion.stats` — `None`, never
guessed, if absent.

### `fn parse_top_tracks(data) -> Vec<TrackSummary>`
Reads `artistUnion.discography.topTracks.items[].track`. Field paths
(`coverArt.sources[]`, `artists.items[].profile.name`) are undocumented —
Pathfinder is private — so every accessor is defensive with multiple
candidate pointers, matching [[home.rs|spotify/home.rs]]'s `image()`
pattern. A wrong or renamed field degrades to an empty result, never a
parse error; [[commands.rs]]'s `get_artist_overview` falls back to the
older REST reconstruction ([[../library.rs|library.rs]]'s `artist_tracks`)
when this comes back empty.

## Dependencies

**Imports:** `serde_json`, [[../library.rs|library.rs]] (`TrackSummary`)
**Imported by:** [[mod.rs|spotify/mod.rs]] (`InternalSpotify::artist_overview`)

## See also

[[pathfinder.rs|spotify/pathfinder.rs]] · [[concerts.rs]] · [[commands.rs]] ·
[[ArtistView.svelte]] · [[known-limitations]] · [[MOC]]
