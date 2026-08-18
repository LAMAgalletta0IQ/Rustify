---
tags: [file, frontend, ui, library]
---
# `src/lib/views/AlbumView.svelte`

Album identity, real album-context playback, track list, and library toggle.
On album change it independently requests simplified album tracks and the real
generic-library saved flag.

The heart starts unknown/disabled, then exposes `aria-pressed`. Mutations are
per-action pending, optimistic, and restore the exact previous state on typed
failure. The backend uses `spotify:album:<id>` with `/me/library`, not the
deprecated `/me/albums` mutation endpoints.

Album track responses omit nested album art, so the header owns the primary
cover while [[TrackList.svelte]] renders placeholders for simplified rows.

## See also

[[library.rs]] · [[TrackList.svelte]] · [[ArtistView.svelte]] · [[MOC]]
