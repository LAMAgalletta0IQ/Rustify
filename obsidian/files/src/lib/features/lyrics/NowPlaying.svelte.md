---
tags: [file, frontend, ui, playback, lyrics]
---
# `src/lib/features/lyrics/NowPlaying.svelte`

Responsive Now Playing view: navigable artwork/identity plus a lyrics-or-queue
panel. [[PlayerBar.svelte]] remains the standard transport surface. Native
fullscreen hides it and adds exactly one local timeline, so the seek control
is never duplicated.

Queue refresh is guarded by the current track URI so periodic playback
snapshots do not refetch the Spotify queue. A queue click replays the visible
queue as an explicit list because Spotify exposes no skip-to-index operation.

Lyrics are fetched once by [[store.svelte.ts]] with a request-generation guard.
The view uses panel-local `scrollTo`, never `scrollIntoView`, so an active
line cannot scroll an ancestor. Wheel, touch, or focused keyboard navigation
enters manual mode; only "Return to current line" or a lyric seek resumes
following. Synced lines expose rounded hover/focus surfaces and seek timestamps.

Fullscreen transitions are awaited and overlap-guarded. Escape exits native
fullscreen but leaves Now Playing mounted and the previous tab intact. Close
leaves the view. Artwork remains navigation in both modes.

## See also

[[mod.rs|lyrics/mod.rs]] · [[queue.rs]] · [[PlayerBar.svelte]] ·
[[2026-08-ui-audio-stability-pass]] · [[MOC]]
