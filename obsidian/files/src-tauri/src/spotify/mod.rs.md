---
tags: [file, backend, pathfinder, rust]
---
# `src-tauri/src/spotify/mod.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **234 lines**

Entry point for Spotify's first-party internal services used by
user-facing features — Home, DJ, concerts, credits, artist stats/top
tracks, generated-playlist tracklists, fuzzy user search. All authenticate
with `first_party_auth`: the same three credentials [[jams_bridge.rs]] uses
for Jam (login5 access token, `client-token`, dealer connection id), pulled
fresh from the live librespot `Session` on every call rather than cached
long-lived — these rotate independently and a stale one fails silently.

## Key items

### `struct InternalSpotify`
Owned by [[state.rs]]'s `AppState`. Holds `PathfinderClient` (one, shared
across every operation below) and `DjClient`. Public methods:
`search_users`, `track_credits`, `artist_overview` (stats + top tracks +
concerts, one Pathfinder call), `home`, `resolve_dj`,
`prepare_dj_narration`, `cached_dj`, `begin_dj_refill`/`finish_dj_refill`,
`playlist_contents`.

### `async fn first_party_auth(session) -> FirstPartyAuth`
The shared credential-fetch every method above starts with.

## Dependencies

**Imports:** `librespot::core::session::Session`, [[pathfinder.rs|spotify/pathfinder.rs]],
`home`, `dj`, `concerts`, `credits`, `users`, `artist_extras`, `playlist_contents`
**Imported by:** [[commands.rs]], [[state.rs]]

## See also

[[pathfinder.rs|spotify/pathfinder.rs]] · [[home.rs|spotify/home.rs]] ·
[[dj.rs|spotify/dj.rs]] · [[jams_bridge.rs]] (the parallel design for Jam) ·
[[MOC]]
