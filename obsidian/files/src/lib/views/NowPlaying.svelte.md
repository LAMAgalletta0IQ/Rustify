---
tags: [file, frontend, ui, playback, lyrics]
---
# `src/lib/views/NowPlaying.svelte`

Three-column now-playing view: artwork/identity, lyrics, and the active-device
queue. Below 900 px the queue moves to a second row. [[PlayerBar.svelte]]
remains the transport surface.

Queue refresh is keyed to track URI. A queue click replays the visible queue as
an explicit list because Spotify exposes no skip-to-index/reorder/remove API.

Lyrics are also keyed to the current track and reset immediately on change.
[[lyrics.rs]] receives exact track, first artist, album, and duration fields.
The view supports loading, synchronized, plain, instrumental, unavailable, and
error states. Synced lines are activated by `playback.positionMs` and scrolled
to the center; both OS reduced motion and Rustify's persisted setting disable
smooth scrolling/line transitions. Stale requests are ignored after cleanup.

LRCLIB is named in the UI as the provider. Spotify/librespot expose no supported
lyrics endpoint; see [[2026-08-capability-audit]].

## See also

[[lyrics.rs]] · [[queue.rs]] · [[PlayerBar.svelte]] · [[known-limitations]] · [[MOC]]
