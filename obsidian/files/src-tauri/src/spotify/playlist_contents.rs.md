---
tags: [file, backend, pathfinder, rust]
---
# `src-tauri/src/spotify/playlist_contents.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **174 lines** · added 2026-08

Pathfinder fallback for playlist tracklists. `/playlists/{id}/items` (the
public REST path [[../library.rs|library.rs]]'s `playlist_tracks` uses)
answers 404 for Spotify-*generated* playlist ids — Daily Mix, Discover
Weekly, Release Radar, Daylist and the rest of the personalized family —
even though the id carries an ordinary `spotify:playlist:` URI everywhere
else (Home cards, `loadContext`, librespot's own Connect playback). This is
the same Pathfinder operation (`fetchPlaylistContents`) the Spotify web
player itself calls for that content.

## Key items

### `fn variables(playlist_id, limit, offset) -> AppResult<Value>`
Validates the id is alphanumeric before building `spotify:playlist:<id>`.

### `fn parse(data) -> AppResult<PlaylistContentsPage>`
Reads `playlistV2.content.items[].itemV2`, keeping only
`TrackResponseWrapper` entries (skips local files and episodes/chapters).
Errors if `playlistV2.content` itself is missing (so the caller knows the
fallback failed too, rather than silently reporting an empty playlist).

## Dependencies

**Imports:** `serde_json`, [[../library.rs|library.rs]] (`TrackSummary`)
**Imported by:** [[mod.rs|spotify/mod.rs]] (`InternalSpotify::playlist_contents`)

## Notable logic / gotchas

> Only tried after the REST call 404s specifically (`AppError::Unavailable`)
> — see [[../commands.rs|commands.rs]]'s `get_playlist_tracks`. Any other
> REST error (auth, rate limit, genuinely missing playlist) is returned
> as-is, not masked by a second, different error from this fallback.

## See also

[[pathfinder.rs|spotify/pathfinder.rs]] · [[../library.rs|library.rs]] ·
[[home.rs|spotify/home.rs]] · [[commands.rs]] · [[known-limitations]] · [[MOC]]
