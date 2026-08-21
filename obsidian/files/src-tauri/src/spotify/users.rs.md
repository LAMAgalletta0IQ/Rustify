---
tags: [file, backend, pathfinder, rust]
---
# `src-tauri/src/spotify/users.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **179 lines**

Fuzzy Spotify user search via Pathfinder's `searchUsers` operation — powers
the profile page's user-search box.

## Key items

### `fn variables(query, limit, offset) -> AppResult<Value>`
Bounds `query` to 200 characters and rejects control characters before it
ever reaches the network.

### `fn parse(data) -> AppResult<UserSearchPage>`
Tolerant of both the wrapped (`{"data": UserResponseWrapper}`) and
unwrapped entity shape — Spotify has changed this envelope independently of
the operation schema before. Caps results at `MAX_USERS` and bounds
image/text field lengths defensively (untrusted search-result content).

## Dependencies

**Imports:** `serde_json` — no crate-internal imports beyond `error.rs`
**Imported by:** [[mod.rs|spotify/mod.rs]] (`InternalSpotify::search_users`)

## See also

[[pathfinder.rs|spotify/pathfinder.rs]] · [[Profile.svelte]] · [[MOC]]
