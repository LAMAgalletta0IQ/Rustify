---
tags: [module, backend]
---
# Module — Backend (Rust)

**Path:** `src-tauri/src/` · **14 files**

The entire backend: audio, authentication, HTTP, state, and the command surface
exposed to the webview.

## Files by role

### Entry and wiring
| File | Role |
| --- | --- |
| [[main.rs]] | OS entry point. 6 lines |
| [[lib.rs]] | Declares modules, builds the Tauri app, registers 43 commands |
| [[state.rs]] | `AppState`, `PlaybackState`, `AuthState`, `TokenStore`, event names |
| [[error.rs]] | `AppError` — the one error type crossing the IPC boundary |

### Feature modules
| File | Role | Backed by |
| --- | --- | --- |
| [[auth.rs]] | OAuth, token storage, refresher, Premium check | librespot-oauth + Web API |
| [[player.rs]] | librespot session, `Spirc`, event pump | librespot |
| [[connect.rs]] | Device list, transfer | Web API |
| [[library.rs]] | Playlists, albums, liked songs, save/unsave, artist | Web API |
| [[mod.rs|lyrics/mod.rs]] | Documented lyrics lookup and synchronized LRC parsing | LRCLIB |
| [[audio-mod]] | CPAL output selection, fallback, presets, and real-time DSP | librespot sink + CPAL |
| [[search.rs]] | Multi-type search | Web API |
| [[queue.rs]] | Queue read/append | Web API |
| [[media_keys.rs]] | Global media-key shortcuts | Tauri plugin |

### Infrastructure
| File | Role |
| --- | --- |
| [[commands.rs]] | Every `#[tauri::command]`; the frontend's whole surface |
| [[webapi.rs]] | Thin Spotify HTTP client: auth header, typed statuses, empty success |

## Internal dependency graph

```
        main.rs
           │
        lib.rs ──────────────► media_keys.rs
           │                        │
      commands.rs ◄─────────────────┘
     ╱  │  │  │  ╲  ╲
auth  player connect library lyrics search queue
  │      │      ╲    │    ╱      ╱
  │      │       webapi.rs ◄────┘
  │      │           │
  └──────┴───────────┴──────► state.rs, error.rs
```

`state.rs` and `error.rs` are leaves — depended on by everything, depending on
nothing. `commands.rs` is the only module the frontend can reach.

## Conventions

- **Errors** — every public fn returns `AppResult<T>`. Commands return
  `AppError`, which serialises as `{kind, message}` so the UI can branch on
  `kind`. See [[error.rs]].
- **Wire vs. summary types** — Web API modules define private `Wire*` structs
  mirroring Spotify's nested JSON, then convert to flat `*Summary` types via
  `From`. Keeps IPC payloads small and Svelte components trivial.
- **`serde(rename_all = "camelCase")`** on everything crossing IPC, matching
  [[types.ts]].
- **Locks** — `tokio::sync::RwLock`, never `std`, because they are held across
  `.await`.
- **Comments explain *why*.** Non-obvious constraints (the `initial_volume`
  scale, calling `get_player_event_channel` before the move) are commented at
  the site.

## Where to start reading

1. [[lib.rs]] — see everything registered in one place.
2. [[state.rs]] — the data the app revolves around.
3. [[commands.rs]] — the full API surface.
4. [[player.rs]] — the most intricate file; the event pump lives here.

## See also

[[architecture]] · [[entry-points]] · [[data-flow]] · [[state-and-events]] ·
[[frontend-svelte]] · [[tauri-config]] · [[MOC]]
