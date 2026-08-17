---
tags: [file, frontend]
---
# `src/lib/api.ts`

**Module:** [[frontend-svelte]] · **Language:** TypeScript · **101 lines**

## Purpose

One typed wrapper per Tauri command — the frontend's entire backend surface.
**No component calls `invoke` directly**, so the IPC contract lives in exactly
one file.

## Key items

### Event name constants
```ts
export const EVENT_PLAYBACK = "playback:changed";
export const EVENT_AUTH     = "auth:changed";
```
Must match `state::events` in [[state.rs]]. Nothing enforces it.

### `asAppError(e: unknown): AppErrorPayload`
Tauri rejects with the serialised `AppError` from [[error.rs]]. This narrows
`unknown` to `{kind, message}`, falling back to
`{kind: "Other", message: String(e)}` for non-conforming throws. Every catch
block in the app goes through it.

### Command wrappers

| Group | Functions |
| --- | --- |
| auth | `getAuthState`, `getLoginInfo`, `login`, `restoreSession`, `logout` |
| playback | `getPlayback`, `play`, `pause`, `playPause`, `nextTrack`, `previousTrack`, `seek`, `setVolume`, `setShuffle`, `setRepeat`, `loadContext`, `loadTracks` |
| connect | `listDevices`, `transferPlayback`, `activateThisDevice` |
| library | `getPlaylists`, `getPlaylistTracks`, `getSavedTracks`, `getSavedAlbums`, `getAlbumTracks`, `setTracksSaved`, `setAlbumsSaved`, `getTracksSaved`, `getArtistTopTracks`, `getArtistAlbums` |
| search | `searchSpotify` |
| queue | `getQueue`, `addToQueue` |

### `likedSongsUri(userId: string): string`
Builds `spotify:user:<id>:collection`. Not a command — a pure helper, because
Liked Songs is a playable context like any other but has no API endpoint
returning its URI. Used by [[Home.svelte]].

## Inputs / outputs / side effects

Every function performs IPC. All return promises that **reject with
`AppErrorPayload`**, never throw synchronously.

## Dependencies

**Imports:** `@tauri-apps/api/core` (`invoke`), [[types.ts]]
**Imported by:** every view and component, plus [[store.svelte.ts]]

## Notable logic / gotchas

> ### The three-way contract
> A command must exist in all three places or it fails **at runtime**:
> 1. `#[tauri::command]` in [[commands.rs]]
> 2. `generate_handler!` in [[lib.rs]]
> 3. A wrapper here
>
> There is no compile-time check in either language.

- **Argument names are camelCase and must match.** Tauri converts `contextUri`
  → `context_uri`. A misspelling yields a `null` parameter rather than an
  error.
- **`invoke<T>` is an unchecked assertion.** The `T` is trusted, not validated;
  if [[types.ts]] drifts from the Rust structs the mismatch surfaces as
  `undefined` fields at render time.
- Optional params (`limit`, `offset`) are passed through as `undefined`, which
  Tauri maps to Rust `None`.
- Arrow-function consts rather than `function` declarations — a stylistic
  choice giving a compact one-line-per-command file.

## See also

[[commands.rs]] · [[lib.rs]] · [[types.ts]] · [[store.svelte.ts]] ·
[[error.rs]] · [[state.rs]] · [[architecture]] · [[frontend-svelte]] · [[MOC]]
