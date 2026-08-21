---
tags: [file, frontend, ui, library]
---
# `src/lib/views/Library.svelte`

Paginated library browser for playlists, albums, Liked Songs, and followed
artists. Section data is cached in component state; Load more respects offset
pagination for collections and cursors for followed artists. Playlist/album
drill-down retains real context playback.

## See also

[[library.rs]] · [[PlaylistView.svelte]] · [[AlbumView.svelte]] · [[MOC]]
