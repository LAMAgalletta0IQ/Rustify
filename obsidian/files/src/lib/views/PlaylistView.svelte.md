---
tags: [file, frontend, ui, library]
---
# `src/lib/views/PlaylistView.svelte`

Playlist detail shared by Home and Search/library drill-down. It pages current
`/playlists/{id}/items` track data, renders [[TrackList.svelte]] with the real
playlist context URI, and appends Load more results without losing the current
page. Context playback starts at the selected track and continues in Spotify's
playlist order.

## See also

[[library.rs]] · [[Home.svelte]] · [[Search.svelte]] · [[TrackList.svelte]] · [[MOC]]
