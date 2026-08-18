---
tags: [file, frontend, ui, artist]
---
# `src/lib/views/ArtistView.svelte`

Artist detail loads three sources independently: canonical artist identity,
ten playable tracks sampled from recent releases, and a paginated albums and
singles grid. A profile failure retains the search result identity; a track
failure does not erase releases, and vice versa.

The track heading is deliberately **"Tracks from recent releases"**. Spotify
removed `/artists/{id}/top-tracks` in February 2026, so Rustify must not label
the fallback as "Popular". The backend fetches up to five recent releases,
walks their album tracks, injects album metadata/art, deduplicates URI, and
stops at ten. This synthesized list plays through `load_tracks`.

Release pages use Spotify's current maximum `limit=10`, offset pagination,
URI deduplication, and a guarded Load more action. The header Play button still
uses the real artist context URI through librespot.

## See also

[[library.rs]] · [[TrackList.svelte]] · [[2026-08-capability-audit]] · [[MOC]]
