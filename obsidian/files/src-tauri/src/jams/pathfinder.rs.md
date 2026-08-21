---
tags: [file, backend, jam, pathfinder, rust]
---
# `src-tauri/src/jams/pathfinder.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **142 lines**

Jam's own, self-contained client for Spotify's Pathfinder GraphQL gateway —
**not** the same code as [[../spotify/pathfinder.rs|spotify/pathfinder.rs]],
which is the app-wide Pathfinder client used for Home/DJ/concerts/credits/
search. This one exists only because the `jams/` module is deliberately
self-contained and imports nothing from the rest of the crate; the two
clients solve the same protocol problem independently rather than sharing
code, which is a real (accepted) duplication — see the app-wide client's own
note for the multi-candidate-hash/live-discovery machinery neither of these
has reason to duplicate further.

## Key items

### `struct PathfinderClient`
Only accepts **persisted queries**: replays the exact `operationName` +
`sha256Hash` captured from the official client — arbitrary GraphQL is
rejected. Hashes live in `JamConfig.pathfinder_hashes` (no defaults — a
wrong hash fails outright) or can be registered at runtime via
`register_hash`.

## Dependencies

**Imports:** `reqwest`, `serde_json` — no crate-internal imports
**Imported by:** [[session.rs]] (jam metadata enrichment, when a hash is configured)

## See also

[[../spotify/pathfinder.rs|spotify/pathfinder.rs]] (the app-wide client,
architecturally the same idea, separately implemented) · [[session.rs]] ·
[[mod.rs|jams/mod.rs]] · [[MOC]]
