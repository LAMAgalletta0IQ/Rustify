---
tags: [module, frontend, ui]
---
# Module — Views

**Path:** `src/lib/views/` · Parent: [[frontend-svelte]]

Full-screen views. Each occupies the main content area; [[PlayerBar.svelte]]
stays visible beneath all of them.

## The views

| View | Shown when | Data source |
| --- | --- | --- |
| [[Setup.svelte]] | `store.setupNeeded` (no Web API Client ID configured yet) | `get_login_info`, `set_client_id` |
| [[Login.svelte]] | `!store.setupNeeded && !store.auth.loggedIn` | `login` command |
| [[Home.svelte]] | `tab === "home"` | Playlists, saved albums, liked songs |
| [[Search.svelte]] | `tab === "search"` | `search_spotify` |
| [[NowPlaying.svelte]] | `nowPlayingOpen` (overlay) | Store + `get_queue` |
| [[AlbumView.svelte]] | Drill-down from search/artist | `get_album_tracks` |
| [[ArtistView.svelte]] | Drill-down from search | Top tracks + albums |

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
    ├── Home.svelte ──► detail (inline, not a separate view)
    ├── Search.svelte
    │   ├── AlbumView.svelte
    │   └── ArtistView.svelte ──► AlbumView.svelte
    └── NowPlaying.svelte           (overlay)
```

> **Asymmetry worth noting:** [[Home.svelte]] renders playlist/album detail
> *inline* via a `detail` object, while [[Search.svelte]] delegates to the
> standalone [[AlbumView.svelte]]. Two different patterns for a similar job —
> Home's predates the extracted views. Consolidating would be a reasonable
> cleanup.

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
