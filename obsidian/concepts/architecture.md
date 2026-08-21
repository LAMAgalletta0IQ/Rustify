---
tags: [concept, architecture]
---
# Architecture

How the whole system fits together. Start here.

## The shape of it

Two processes, one binary:

```
┌─────────────────────────────────────────────────────────┐
│ Rustify.exe  (Rust, ~5.9 MB private)                │
│                                                          │
│  ┌────────────┐   Tauri commands    ┌─────────────────┐  │
│  │ commands.rs│ ◄──────────────────  │                 │  │
│  │            │ ──────────────────► │  WebView2       │  │
│  │            │   Tauri events      │  (Svelte 5 UI)  │  │
│  └─────┬──────┘                     └─────────────────┘  │
│        │                                                  │
│   ┌────┴─────┬──────────┬─────────┐                      │
│   ▼          ▼          ▼         ▼                      │
│ auth.rs   player.rs  webapi.rs  media_keys.rs            │
│   │          │          │                                 │
└───┼──────────┼──────────┼─────────────────────────────────┘
    │          │          │
    ▼          ▼          ▼
 Spotify    librespot   Spotify
  OAuth     (audio +    Web API
            Connect)   (metadata)
```

The webview never talks to Spotify. Every network call, every credential, and
all audio live in Rust. The frontend only sends commands and receives state.

## The three external systems

| System | Used for | Where |
| --- | --- | --- |
| **librespot OAuth** | Login; issues the access + refresh token | [[auth.rs]] |
| **librespot `Spirc`** | Audio playback *and* being a Connect device | [[player.rs]] |
| **Spotify Web API** | All metadata: library, search, devices, queue, covers | [[webapi.rs]] |

A deliberate design choice ties the first two to the third: **one OAuth login
produces one token used for both librespot and the Web API**. See
[[auth-and-tokens]].

## Backend layering

Requests flow strictly downward; nothing lower reaches back up.

1. **Command layer** — [[commands.rs]]. Every `#[tauri::command]`. Its only
   jobs are unwrapping app state, converting errors, and delegating. It holds
   no logic of its own.
2. **Feature modules** — [[auth.rs]], [[player.rs]], [[connect.rs]],
   [[library.rs]], [[search.rs]], [[queue.rs]], [[media_keys.rs]].
3. **Infrastructure** — [[webapi.rs]] (HTTP), [[state.rs]] (shared state),
   [[error.rs]] (one error type crossing the IPC boundary).

[[lib.rs]] wires all of it into the Tauri builder; [[main.rs]] is a
three-line shim. See [[entry-points]].

### Two kinds of feature module

Worth internalising, because it explains most of the codebase:

- **`player.rs` drives librespot.** It owns the audio pipeline and the Connect
  device identity. Playback control goes here.
- **`connect.rs`, `library.rs`, `search.rs`, `queue.rs` are Web API clients.**
  They are thin: build a URL, deserialise, flatten into a UI-shaped struct.

So "pause" goes through librespot, but "which devices exist" goes through HTTP —
even though both are conceptually "playback". [[playback-and-connect]] explains
why.

## Frontend layering

1. **Shell** — [[App.svelte]] picks between login, the tab views, and the
   now-playing overlay, and always renders [[PlayerBar.svelte]].
2. **Store** — [[store.svelte.ts]], a Svelte 5 runes class holding
   `playback` and `auth`. Subscribes to backend events; the single source of
   truth for the UI.
3. **API layer** — [[api.ts]], one typed wrapper per Tauri command. No
   component calls `invoke` directly.
4. **Types** — [[types.ts]], mirroring the Rust `serde` shapes.

## The boundary contract

### Feature ownership after the stability pass

Route/detail pages remain in `src/lib/views`. Composed behavior now lives with
its domain: `features/audio` (CPAL devices), `features/player` (persistent
transport and Spotify Connect), `features/lyrics` (Now Playing/fullscreen), and
`features/settings`. Shared overlay behavior lives in `src/lib/ui`.

The Rust module paths stay stable while the two multi-responsibility domains
use folders: `audio/mod.rs` and `lyrics/mod.rs`. The conventional one-file Rust
domains remain flat. Experimental Jam modules were deliberately excluded from
this restructuring.

Three things must stay in sync across the Rust/TypeScript line. Change one,
change all:

| Rust | TypeScript | Risk if mismatched |
| --- | --- | --- |
| `#[tauri::command]` name in [[lib.rs]] | wrapper in [[api.ts]] | Runtime "command not found" |
| `serde(rename_all = "camelCase")` structs | interfaces in [[types.ts]] | Silent `undefined` fields |
| `events::PLAYBACK` / `events::AUTH` in [[state.rs]] | constants in [[api.ts]] | UI silently stops updating |

None of these are checked by a compiler. They are the most likely source of
bugs when extending the app.

## Why these choices

- **No Electron.** Tauri uses the OS's WebView2, already present on Windows 11.
  Result: a 13 MB binary instead of a ~300 MB install.
- **Metadata from the Web API, not librespot.** librespot exposes track data via
  `AudioItem`, but the Web API returns ready-to-use CDN cover URLs and a stable,
  documented shape. See [[player.rs]].
- **`Spirc` instead of raw `Player`.** Local and remote control then share one
  path, so state cannot diverge. See [[playback-and-connect]].

## See also

[[entry-points]] · [[data-flow]] · [[state-and-events]] · [[auth-and-tokens]] ·
[[playback-and-connect]] · [[external-dependencies]] · [[known-limitations]] ·
[[backend-rust]] · [[frontend-svelte]] · [[MOC]]
