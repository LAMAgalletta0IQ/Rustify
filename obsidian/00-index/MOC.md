---
tags: [moc, index]
---
# Map of Content — spotify-rust

Everything in this vault is reachable from here.

**spotify-rust** is a native, lightweight Spotify client for Windows. A Rust
backend inside [Tauri 2](https://tauri.app) embeds the
[librespot](https://github.com/librespot-org/librespot) crate for login and
audio; a Svelte 5 frontend renders the UI in a WebView2 webview. Metadata comes
from the official Spotify Web API.

> Start with [[architecture]] if you are new. Then [[entry-points]] to see how
> the app boots, then [[data-flow]] to see how a click becomes audio.

---

## Concepts — read these first

| Note | What it covers |
| --- | --- |
| [[architecture]] | How the backend, frontend, librespot and Web API fit together |
| [[entry-points]] | How the app starts, both processes, and in what order |
| [[data-flow]] | Click → command → librespot → event → UI, traced end to end |
| [[state-and-events]] | The single `PlaybackState` snapshot and how it is pushed |
| [[auth-and-tokens]] | One OAuth login serving both librespot and the Web API |
| [[playback-and-connect]] | Why `Spirc` is the core primitive; Connect in and out |
| [[external-dependencies]] | librespot, Tauri, Svelte, reqwest, the Web API |
| [[build-and-config]] | Toolchain, build commands, the pinned `vergen` dependency |
| [[known-limitations]] | Jams, Blends, lyrics, queue reorder — and *why* |

## Modules

| Module | Contents |
| --- | --- |
| [[backend-rust]] | The 13 Rust source files in `src-tauri/src/` |
| [[frontend-svelte]] | The Svelte 5 app shell and its shared library |
| [[frontend-views]] | Full-screen views: login, home, search, now-playing, album, artist |
| [[frontend-components]] | Reusable UI: player bar, device picker, track list |
| [[tauri-config]] | `tauri.conf.json`, capabilities, `Cargo.toml`, build script |
| [[project-root]] | Root-level config: Vite, TypeScript, npm, git |
| [[icons]] | Generated application icon set |

---

## All file notes by area

### Backend — Rust (`src-tauri/src/`) `#backend`
Entry and wiring: [[main.rs]] · [[lib.rs]] · [[state.rs]] · [[error.rs]]
Feature modules: [[auth.rs]] · [[player.rs]] · [[commands.rs]] · [[connect.rs]] · [[library.rs]] · [[search.rs]] · [[queue.rs]] · [[media_keys.rs]] · [[webapi.rs]]

### Frontend — Svelte (`src/`) `#frontend`
Shell & bootstrap: [[main.ts]] · [[App.svelte]] · [[app.css]] · [[vite-env.d.ts]]
Shared library: [[api.ts]] · [[types.ts]] · [[store.svelte.ts]]
Views: [[Login.svelte]] · [[Home.svelte]] · [[Search.svelte]] · [[NowPlaying.svelte]] · [[AlbumView.svelte]] · [[ArtistView.svelte]]
Components: [[PlayerBar.svelte]] · [[DevicePicker.svelte]] · [[TrackList.svelte]]

### Tauri configuration (`src-tauri/`) `#config`
[[tauri.conf.json]] · [[Cargo.toml]] · [[Cargo.lock]] · [[build.rs]] · [[default.json]]

### Root configuration `#config`
[[package.json]] · [[package-lock.json]] · [[vite.config.ts]] · [[svelte.config.js]] · [[tsconfig.json]] · [[index.html]] · [[.gitignore]] · [[README.md]] · [[app-icon.png]]

---

## Housekeeping

- [[excluded]] — what was left out of this vault and why.

## Tag legend

`#backend` `#frontend` `#config` `#entrypoint` `#state` `#auth` `#playback`
`#webapi` `#ui` `#build` `#generated` `#concept` `#module` `#file`
