---
tags: [file, frontend, ui, personalization]
---
# `src/lib/views/ForYou.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **~24 lines (dense, single-line-per-section)**

## Purpose

"For you" tab: the user's own top tracks (`getTopTracks(40)`) and top artists
(`getTopArtists(20)`) from the Web API, framed explicitly as *not* an
official Spotify mix or editorial playlist — see the on-page copy and
[[known-limitations]]'s "Algorithmic mixes/Daily Mix" section for why that
disclaimer exists (there is no access to Spotify's real recommendation
models, so this page can only ever reflect the user's own listening
history, not a curated suggestion).

## Key items

### `brokenImages` / `onArtworkError`
Same defense-in-depth pattern used across every other artwork-bearing view
(see [[Home.svelte]], [[Search.svelte]], [[Library.svelte]]): a track or
artist image URL that fails to *load* (CSP block, expired CDN link, 404)
falls back to the placeholder `<span class="art">` instead of showing the
WebView's native broken-image icon. Applied to both the tracks grid and the
artists grid.

### Track/artist grids
Clicking a track plays it via `loadTracks` seeded from the whole visible
list (so the rest of the top-40 becomes the queue); clicking an artist opens
`ArtistView` in place, same drill-down pattern as Search/Library.

## Inputs / outputs / side effects

Calls `getTopTracks`, `getTopArtists` once on mount. No polling, no writes.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[types.ts]], [[ArtistView.svelte]]
**Imported by:** [[App.svelte]] (`forYou` tab, reached via a nav button and
via `Home`'s `onOpenForYou`)

## Notable logic / gotchas

> ### Artwork fallback added (2026-08)
> Originally neither grid had an `onerror` handler, so a CSP-blocked or
> dead image URL rendered the WebView's native broken-image glyph instead of
> the app's placeholder tile — the same "Made For You artwork universally
> broken"-class bug documented in [[Home.svelte]] and [[known-limitations]].
> Fixed by applying the same `brokenImages: Set<string>` pattern used
> elsewhere in the app.

## See also

[[Home.svelte]] · [[ArtistView.svelte]] · [[api.ts]] · [[App.svelte]] ·
[[frontend-views]] · [[MOC]]
