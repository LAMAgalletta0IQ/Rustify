---
tags: [file, frontend, ui, releases]
---
# `src/lib/views/Releases.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **~114 lines (dense, single-line-per-section)**

## Purpose

Recent releases from artists the user follows — albums and singles, paged by
cursor, filterable by release type (album/single/compilation) and by date
range (30/90/365 days or any date). This is the client-side substitute for
Spotify's own "New releases" surface: it can only walk `getFollowedReleases`
(cursor-paged by followed-artist id, see `library.rs`'s `/me/following`
cursor note in the top-level CLAUDE.md) rather than any editorial feed.

## Key items

### `loadMore(reset)`
Pages through followed artists one cursor step at a time via
`api.getFollowedReleases`; `page.nextArtist` becomes the next cursor and
`page.partialErrors` accumulates into `partial` so a banner can surface
"some artist catalogs could not be refreshed" without failing the whole
page. Results are de-duplicated by URI and re-sorted by release date on
every page, since pages arrive artist-by-artist rather than
chronologically.

### `visible` (derived)
Client-side filter over the already-loaded `releases` array — `type` and
`range` never trigger a new request, only re-filter what's cached.

### `brokenImages` / `onArtworkError`
Same defense-in-depth artwork-fallback pattern as [[Home.svelte]],
[[Search.svelte]], [[Library.svelte]], [[ForYou.svelte]]: a release cover
that fails to load falls back to the placeholder tile instead of the
WebView's native broken-image icon.

## Inputs / outputs / side effects

Calls `getFollowedReleases`, `getAlbumsSaved`, `setAlbumsSaved`,
`loadContext`. No polling — the page only refetches on mount and on
"Load more".

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[types.ts]], `SelectMenu`
(see [[SelectMenu.svelte]]), [[AlbumView.svelte]]
**Imported by:** [[App.svelte]] (`releases` tab, reached via a nav button and
via `Home`'s `onOpenReleases`)

## Notable logic / gotchas

> ### Artwork fallback added (2026-08)
> The releases grid had no `onerror` handler on its cover images, so a
> CSP-blocked or expired cover URL rendered the WebView's native
> broken-image glyph. Fixed with the same `brokenImages: Set<string>`
> pattern used everywhere else in the app; see [[known-limitations]] for the
> CSP `img-src` fix that made most of these URLs load in the first place.

Cursor semantics are easy to get backward here: `cursor === undefined` means
"never loaded", `cursor === null` means "exhausted, no more artists", and
any string value is the next `after=` cursor to send. The Retry button
passes `cursor === undefined` back into `loadMore` deliberately, so retrying
from a cold state re-runs the initial load rather than resuming a partial
one.

## See also

[[library.rs]] · [[Home.svelte]] · [[AlbumView.svelte]] · [[api.ts]] ·
[[App.svelte]] · [[frontend-views]] · [[MOC]]
