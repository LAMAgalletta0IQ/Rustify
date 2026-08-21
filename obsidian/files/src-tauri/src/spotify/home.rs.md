---
tags: [file, backend, pathfinder, home, rust]
---
# `src-tauri/src/spotify/home.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **338 lines**

Parses Pathfinder's `home` operation into `HomeFeed`/`HomeSection`/
`HomeItem` — Daily Mix, Discover Weekly, Release Radar, Daylist and the rest
of the personalized-shelf family, plus ordinary playlist/album/artist
shelves, from one response shape.

## Key items

### `enum HomePersonalization`
Stable, **non-localized** classification (`DailyMix`, `DiscoverWeekly`,
`ReleaseRadar`, `Daylist`, `ArtistMix`, `TopicMix`, `InspiredByMix`,
`MadeForYou`) derived from Spotify's own `format`/attribute fields, never
from matching a display title — titles are localized and change.

### `fn parse_item(item) -> Option<HomeItem>`
Forward-compatible: known media types get normalized fields
(name/subtitle/image/personalization) while the original `__typename` and
raw attributes stay available for capability detection.

### `fn image(data) -> Option<String>`
Tries four candidate pointers in order — `/images/items/0/sources/0/url`,
`/coverArt/sources/0/url`, `/visuals/avatarImage/sources/0/url`,
`/coverImage/sources/0/url` — since different Home entity types represent
artwork under different fields, and rejects anything not `http(s)://`.

## Dependencies

**Imports:** `serde_json` — no crate-internal imports beyond `error.rs`
**Imported by:** [[mod.rs|spotify/mod.rs]] (`InternalSpotify::home`)

## Notable logic / gotchas

> ### A generated-playlist card 404s on its own tracklist
> A Daily Mix/Discover Weekly/etc. card's `uri` looks like an ordinary
> `spotify:playlist:...` everywhere — this file, `loadContext`, librespot's
> own Connect playback — but the public REST `/playlists/{id}/items`
> answers 404 for these ids specifically, because they aren't real playlist
> objects the public API exposes to third-party apps. That's not a bug in
> *this* file (which only builds the card), but the reason
> [[playlist_contents.rs]] exists: `get_playlist_tracks` retries through
> Pathfinder's `fetchPlaylistContents` on exactly that 404.

## See also

[[pathfinder.rs|spotify/pathfinder.rs]] · [[playlist_contents.rs]] ·
[[Home.svelte]] · [[known-limitations]] · [[MOC]]
