---
tags: [file, backend, pathfinder, rust]
---
# `src-tauri/src/spotify/pathfinder.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **475 lines**

The app-wide Pathfinder GraphQL client (`api-partner.spotify.com/pathfinder/v2/query`),
used by Home, DJ, concerts, credits, artist stats/top tracks, generated-
playlist tracklists and fuzzy user search. **Not** the same code as
[[../jams/pathfinder.rs|jams/pathfinder.rs]] — that one is Jam's own,
independently implemented copy, kept separate on purpose because `jams/` is
self-contained. The centerpiece here is persisted-query hash resilience:
Pathfinder only accepts a known `operationName` + `sha256Hash` pair, Spotify
rotates these without notice, and this file is what stops that from being a
recurring outage.

## Key items

### `struct PathfinderClient { http, discovered, discovery_gate }`
### `async fn query(operation, variables, access_token, client_token, connection_id) -> AppResult<Value>`
Tries candidate hashes in order (most-recently-discovered first, then an
optional `RUSTIFY_PATHFINDER_<OP>_HASH` env override, then the hardcoded
"recently observed" list per operation); on `QueryFailure::HashRejected`
from every candidate, falls through to `discover_hash` and retries once.

### `async fn discover_hash(operation) -> AppResult<String>`
Scrapes the *currently served* Spotify Web Player bundle: fetches
`open.spotify.com`, finds the main `web-player.*.js`/`mobile-web-player.*.js`
script, searches it for the operation's hash, and — if not found there —
downloads and searches webpack chunks (sorted to check `home`/`shelf`-named
ones first, capped at 192) until one contains it.

### Known operations and their hardcoded candidate hashes
`home`, `queryArtistOverview` (also serves concerts/stats/top-tracks —
[[artist_extras.rs]], [[concerts.rs]]), `queryTrackCreditsModal`
([[credits.rs]]), `searchUsers` ([[users.rs]]), `fetchPlaylistContents`
([[playlist_contents.rs]] — added 2026-08, its one candidate hash is
community-sourced and unverified against a live account; wrong is safe,
it just falls through to live discovery like any other operation).

## Dependencies

**Imports:** `reqwest`, `regex`, `serde_json`, `tokio::sync`
**Imported by:** [[mod.rs|spotify/mod.rs]]

## Notable logic / gotchas

> ### `queryArtistOverview` returns far more than concerts
> The same response (`data.artistUnion`) that [[concerts.rs]] reads
> `goods.concerts` from also carries `stats.monthlyListeners`/`followers`
> and `discography.topTracks` — [[artist_extras.rs]] parses those from the
> identical payload `InternalSpotify::artist_overview` already fetches once,
> which is how artist top-tracks stopped needing a five-album REST fan-out.

> ### 403/404 vs. a genuinely wrong hash
> `persisted_query_not_found(&body)` and `StatusCode::PRECONDITION_FAILED`
> both mean the hash itself was rejected (triggers rediscovery); a 403/404
> on an otherwise-accepted query means something else (permission, missing
> resource) and is surfaced as `AppError::Forbidden`/`Unavailable` instead —
> don't conflate the two when adding a new operation.

## See also

`README_jams.md` (the equivalent resilience story for `jams/pathfinder.rs`,
told independently) · [[artist_extras.rs]] · [[concerts.rs]] ·
[[playlist_contents.rs]] · [[home.rs|spotify/home.rs]] · [[MOC]]
