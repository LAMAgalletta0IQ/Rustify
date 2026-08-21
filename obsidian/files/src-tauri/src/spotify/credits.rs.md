---
tags: [file, backend, pathfinder, rust]
---
# `src-tauri/src/spotify/credits.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **178 lines**

Parses track credits (performers/writers/producers, record label) from
Pathfinder's `queryTrackCreditsModal` operation — powers the credits modal
reachable from a track's context menu.

## Key items

### `fn parse(data) -> AppResult<TrackCredits>`
Reads `creditsTrait.sources.items` (contributor entries) and groups by
`roleGroup.name`, deduplicating contributors who appear under multiple
roles.

## Dependencies

**Imports:** `serde_json` — no crate-internal imports beyond `error.rs`
**Imported by:** [[mod.rs|spotify/mod.rs]] (`InternalSpotify::track_credits`)

## See also

[[pathfinder.rs|spotify/pathfinder.rs]] · [[commands.rs]] · [[MOC]]
