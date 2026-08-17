---
tags: [concept, limitations]
---
# Known limitations

Deliberate gaps, each with its cause. None of these are bugs to fix in this
codebase — most are upstream constraints.

## Not supported at all

### Jams
Spotify's real-time collaborative listening. **No public Web API exists** and
librespot implements nothing for it. Omitted entirely rather than mocked with
a disabled button.

### Blends — read/play only
An existing Blend is an ordinary playlist: it appears in [[library.rs]]'s
playlist list and plays like any other. **Creating** a Blend or inviting a
participant uses a private, undocumented endpoint and is not implemented.

### Lyrics
No public Web API endpoint. [[NowPlaying.svelte]] deliberately leaves layout
space for a future provider, with a comment marking the spot.

### Free-tier playback
librespot cannot play the ad-supported tier. Enforced early with a clear
message — see [[auth-and-tokens]].

## API-limited

### Queue reorder / remove
`POST /me/player/queue` appends. The Web API offers **no reorder or remove
operation**. [[queue.rs]] can only read and append; jumping into the queue is
implemented as replaying it as an explicit track list from the chosen entry.

### Queue reflects the active device
`GET /me/player/queue` returns the **active** device's queue. Accurate while
this app is active — the normal case — but it can show another device's queue
after a transfer.

### Artist top tracks have no context
`/artists/{id}/top-tracks` returns a synthesised list, not a browsable Spotify
context, so [[ArtistView.svelte]] plays them via `load_tracks` rather than
`load_context`.

### Album rows have no cover art
`/albums/{id}/tracks` returns *simplified* track objects with no nested album,
so `imageUrl` is null on every row. [[AlbumView.svelte]] shows the art in the
header instead.

## Implementation heuristics

### `is_active_device` is inferred
librespot exposes no explicit flag. [[player.rs]] infers it from player events.
Reliable in practice, but a heuristic. See [[state-and-events]].

### The position ticker can drift
[[store.svelte.ts]] ticks position locally at 1 Hz between backend updates.
Between `PositionCorrection` events it may drift by a fraction of a second. A
deliberate CPU trade-off.

### Media keys are best-effort
Media keys are a globally exclusive OS resource. If another player already
holds them, [[media_keys.rs]] logs a warning and continues rather than failing
startup — so they can silently not work. Check the log before assuming a bug.

## Scope decisions

### Windows only
No cross-platform abstractions. The build targets `x86_64-pc-windows-msvc`;
`bundle.targets` is `nsis`. Icon generation produced iOS/Android assets as a
side effect of `tauri icon` — they are unused. See [[icons]].

### No pagination beyond "Load more"
[[Home.svelte]] appends pages on demand. No virtual scrolling, so very large
libraries will accumulate DOM nodes.

### Search results are not paginated
[[Search.svelte]] requests a single page (default 20 per type).

### No offline mode
librespot caches audio (2 GB cap) but there is no offline UI; all metadata
requires network.

## Untested surface

As of this vault, the app **compiles, links, and launches**, and the frontend
type-checks. Not yet exercised against a live Premium account: login,
playback, Connect registration, and every Web API call. The manual checklist in
[[README.md]] covers them in order.

Highest residual risk is Connect/`Spirc` behaviour — the piece with the least
verifiable surface without a real account and a second device.

## See also

[[architecture]] · [[playback-and-connect]] · [[auth-and-tokens]] ·
[[state-and-events]] · [[external-dependencies]] · [[README.md]] · [[MOC]]
