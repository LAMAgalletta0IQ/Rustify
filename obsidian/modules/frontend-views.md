---
tags: [module, frontend, ui]
---
# Module — Views

**Path:** `src/lib/views/` · Parent: [[frontend-svelte]]

Full-screen views. Each occupies the main content area; [[PlayerBar.svelte]]
stays visible beneath all of them.

`NowPlaying` and `Settings` moved to feature folders because they coordinate
domain state and shared components rather than acting as independent routes.

## The views

| View | Shown when | Data source |
| --- | --- | --- |
| [[Setup.svelte]] | `store.setupNeeded` (no Web API Client ID configured yet) | `get_login_info`, `set_client_id` |
| [[Login.svelte]] | `!store.setupNeeded && !store.auth.loggedIn` | `login` command |
| [[Home.svelte]] | `tab === "home"` | Recent contexts, top tracks/artists, releases, friend activity rail, playlist fallback |
| [[Search.svelte]] | `tab === "search"` | `search_spotify` |
| [[Library.svelte]] | `tab === "library"` | Saved collections/followed artists |
| [[ForYou.svelte]] | `tab === "forYou"` | The user's own top tracks/artists (`get_top_tracks`/`get_top_artists`), explicitly not framed as an editorial mix |
| [[Releases.svelte]] | `tab === "releases"` | Cursor-paged followed-artist releases (`get_followed_releases`), filterable by type/date |
| [[Jams.svelte]] | `tab === "jams"`, reached from the player bar's Jam button | Spotify Jam lifecycle — create/join/leave/kick/end, live via dealer events |
| [[Settings.svelte]] | `tab === "settings"` | Persisted functional settings + integration status |
| [[Profile.svelte]] | Account button | Current session, top items, playlists |
| [[NowPlaying.svelte]] | `nowPlayingOpen` (overlay) | Store + queue + LRCLIB lyrics |
| [[PlaylistView.svelte]] | Drill-down from Home/Search/Library | `get_playlist_tracks` (with a Pathfinder `fetchPlaylistContents` fallback on 404) |
| [[AlbumView.svelte]] | Drill-down from search/artist/playlist/releases | `get_album_tracks` |
| [[ArtistView.svelte]] | Drill-down from search/Home | `get_artist_overview` (stats + top tracks + concerts) + paged discography |

## Navigation

Two independent mechanisms, which is worth knowing before adding a view:

**Top level** — [[App.svelte]] owns `tab` and `nowPlayingOpen`. `NowPlaying`
is an overlay: it *replaces* the tab content rather than stacking over it.

**Drill-down** — [[Search.svelte]] owns `openAlbum` / `openArtist` and renders
those views *instead of* its results, keeping query and results intact behind
them. Artist → album re-parents by clearing `openArtist` and setting
`openAlbum`.

```
App.svelte
├── Setup.svelte                    (no Client ID configured yet)
├── Login.svelte                    (logged out)
└── logged in
    ├── Home.svelte ──► Playlist / Album / Artist views
    ├── Search.svelte
    │   ├── AlbumView.svelte
    │   └── ArtistView.svelte ──► AlbumView.svelte
    ├── Library.svelte ──► Playlist / Album / Artist views
    ├── ForYou.svelte ──► ArtistView.svelte
    ├── Releases.svelte ──► AlbumView.svelte
    ├── Jams.svelte                 (reached from PlayerBar's Jam button, not the tab bar)
    ├── Settings.svelte
    ├── Profile.svelte              (account area)
    └── NowPlaying.svelte           (overlay with lyrics/queue)
```

Home and Search both reuse the standalone album/artist/playlist detail views;
their local state only decides which drill-down is active.

## Shared patterns

**Async load in `$effect` + `untrack`** — see [[frontend-svelte]] for why.

**Cancellation flag** — [[AlbumView.svelte]] and [[ArtistView.svelte]] set
`cancelled = true` in the effect's cleanup so a slow response for a previous
item cannot overwrite newer state.

**Local `loading`** — each view owns its own flag rather than a global spinner.

**Errors to the banner** — `store.error = api.asAppError(e).message`, except
[[Login.svelte]], which needs the `PremiumRequired` branch.

**Play buttons load a context** — `api.loadContext(uri)` on the container, so
playback continues past the first track. See [[playback-and-connect]].

## See also

[[frontend-svelte]] · [[frontend-components]] · [[App.svelte]] ·
[[data-flow]] · [[MOC]]
