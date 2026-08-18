---
tags: [file, frontend, state]
---
# `src/lib/types.ts`

**Module:** [[frontend-svelte]] · **Language:** TypeScript · **118 lines**

## Purpose

TypeScript mirrors of every Rust struct that crosses the IPC boundary, plus two
small formatting helpers. The file header states the contract explicitly: keep
in sync with `src-tauri/src/{state,library,search,connect,queue}.rs`.

## Interfaces and their Rust counterparts

| TypeScript | Rust | Defined in |
| --- | --- | --- |
| `AppErrorPayload` | `AppError` (custom `Serialize`) | [[error.rs]] |
| `TrackInfo` | `TrackInfo` | [[state.rs]] |
| `PlaybackState` | `PlaybackState` | [[state.rs]] |
| `AuthState` | `AuthState` | [[state.rs]] |
| `Device` | `Device` | [[connect.rs]] |
| `TrackSummary`, `PlaylistSummary`, `AlbumSummary` | same | [[library.rs]] |
| `AlbumPage`, `RecentActivityItem` | same | [[library.rs]] |
| `ArtistSummary`, `PlaylistHit`, `SearchResults` | same | [[search.rs]] |
| `QueueView` | `QueueView` | [[queue.rs]] |
| `LyricsResult`, `LyricsLine` | same | [[lyrics.rs]] |
| `AppSettings` | `commands::AppSettings` | [[commands.rs]] |

`AppErrorPayload["kind"]` is a **string-literal union** matching `AppError::kind`,
so branching on it is exhaustively checked in TypeScript even though nothing
verifies it against Rust. It also carries
`retryAfter?: number | null` — populated only for `RateLimited`, and used by
[[Login.svelte]] to drive its countdown. See [[rate-limiting]].

## Helpers

### `MAX_VOLUME = 65535`
### `interface LoginInfo`
`privateClientId`, `clientIdEnv`, `webapiRedirectUri` — mirrors
`commands::LoginInfo`. Lets [[Login.svelte]] describe the login flow and print
the dashboard steps without hardcoding values owned by [[auth.rs]].

### `interface PlaybackState`
Mirrors the Rust struct, **minus** `position_base_ms` / `position_at`: those are
`#[serde(skip)]` and never cross the IPC boundary. The UI sees only the
already-recomputed `positionMs`. See [[state.rs]].

### `volumeToPercent(v: number): number`
Converts librespot's raw scale to `0..100` for display. The inverse,
`percent_to_volume`, lives in [[player.rs]] — conversion happens at both
boundaries, never in between.

### `formatMs(ms: number): string`
`m:ss`, guarding against `NaN`/negative input by returning `"0:00"`. Used by
[[PlayerBar.svelte]], [[TrackList.svelte]] and [[NowPlaying.svelte]].

## Inputs / outputs / side effects

Pure. Interfaces are erased at compile time; only the two functions and the
constant emit code.

## Dependencies

**Imports:** none
**Imported by:** [[api.ts]], [[store.svelte.ts]], and every view and component

## Notable logic / gotchas

- **`Device.type` is a reserved-ish field name.** Rust names it `device_type`
  and renames it to `type` with `#[serde(rename = "type")]`; the TS interface
  must use `type`, not `deviceType`.
- **Optional Rust fields map to `| null`, not `?`.** `serde` emits explicit
  `null` for `Option::None`, so `coverUrl: string | null` is correct and
  `coverUrl?: string` would be wrong.
- **`volume` is raw, not a percentage.** Rendering `PlaybackState.volume`
  directly on a 0–100 slider would peg it at maximum. See
  [[playback-and-connect]] for the matching backend hazard.
- Drift here is silent: a renamed Rust field just becomes `undefined` at
  runtime. TypeScript cannot catch it.

## See also

[[api.ts]] · [[state.rs]] · [[error.rs]] · [[library.rs]] · [[search.rs]] ·
[[connect.rs]] · [[queue.rs]] · [[state-and-events]] · [[frontend-svelte]] ·
[[MOC]]
