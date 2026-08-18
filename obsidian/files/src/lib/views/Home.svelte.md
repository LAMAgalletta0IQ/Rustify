---
tags: [file, frontend, ui, home]
---
# `src/lib/views/Home.svelte`

Home is a landing surface with three independent real-data shelves and a
ranked quick-access grid.

## Data and ranking

- `get_recently_played(50)` returns [[library.rs]]'s context-aware
  `RecentActivityItem`; Home displays 12. Album/playlist/artist contexts are one
  item each, while loose tracks remain tracks.
- Quick access takes the newest distinct recent contexts first, then fills to
  six with the stable order from the user's playlist library. Known playlist
  IDs are enriched with their real title/owner/art.
- "Your listening mix" is the account's `/me/top/tracks` data, explicitly
  labeled as a locally assembled history view rather than Spotify editorial
  content.
- "From your top artists" combines `/me/top/artists` with supported artist
  release pages, deduplicates album URIs, and makes no recommendation claim.

Each source has its own loading, empty, error, retry, and cancellation state;
one forbidden/offline endpoint does not blank the others. Spotify itself
persists history/top-item state, so Rustify does not maintain a competing local
history database.

Quick items drill into known albums/artists/playlists. Unknown playlist
contexts remain playable. Loose tracks and the top-track shelf use
`load_tracks`; contexts use `load_context` with the most recently played track
as their starting point.

## See also

[[2026-08-capability-audit]] · [[library.rs]] · [[ArtistView.svelte]] ·
[[AlbumView.svelte]] · [[PlaylistView.svelte]] · [[MOC]]
