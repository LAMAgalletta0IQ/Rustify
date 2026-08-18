---
tags: [concept, dependencies]
---
# External dependencies and integrations

What the project depends on and how each is used.

## Rust crates ([[Cargo.toml]])

| Crate | Version | Role |
| --- | --- | --- |
| `librespot` | 0.8.0 | Audio, login, Connect. The heart of the app |
| `librespot-oauth` | 0.8.0 | Loopback OAuth flow |
| `tauri` | 2 | Window, IPC, app data paths |
| `tauri-plugin-opener` | 2 | Opens the browser for OAuth |
| `tauri-plugin-global-shortcut` | 2 | Media keys |
| `reqwest` | 0.12 | HTTP for Spotify and documented LRCLIB reads (`json`, `native-tls`) |
| `tokio` | 1 | Async runtime (shared with Tauri) |
| `serde` / `serde_json` | 1 | Serialisation across IPC and HTTP |
| `thiserror` | 2 | Error derive |
| `log` / `env_logger` | 0.4 / 0.11 | Logging |
| `dotenvy` | 0.15 | Loads `.env` before the logger — see [[build-and-config]] |
| `futures-util` | 0.3 | Async utilities |

### librespot feature selection

```toml
librespot = { version = "0.8.0", default-features = false,
              features = ["native-tls", "rodio-backend"] }
```

`default-features = false` is deliberate. The dropped default is
**`with-libmdns`** — zeroconf discovery, only needed for the "log in from
another device" flow. This app authenticates by OAuth and registers as a
Connect device through Spotify's dealer/websocket, so mDNS is dead weight.

`rodio-backend` gives WASAPI output on Windows.

### The librespot API surface actually used

| Item | Where | Purpose |
| --- | --- | --- |
| `OAuthClientBuilder` | [[auth.rs]] | Login, refresh |
| `SessionConfig::default().client_id` | [[auth.rs]] | Spotify's desktop client ID — streaming only; Web API uses `RUSTIFY_CLIENT_ID` |
| `Credentials::with_access_token` | [[commands.rs]] | Streaming credentials. `with_password` also exists but Spotify disabled it server-side in 2024 — see [[known-limitations]] |
| `Session`, `Cache` | [[player.rs]] | Connection, audio/credential cache |
| `Player`, `PlayerEvent` | [[player.rs]] | Audio; the event stream driving the UI |
| `Spirc`, `ConnectConfig`, `LoadRequest`, `PlayingTrack` | [[player.rs]], [[commands.rs]] | Connect device + transport |
| `mixer`, `audio_backend` | [[player.rs]] | Volume, output sink |
| `SpotifyUri` | [[player.rs]] | `to_uri()`, `to_id()`, `item_type()` |

> **Version sensitivity.** librespot's library API changes across minor
> versions and its docs lag the source. When something disagrees, read the
> vendored source under `~/.cargo/registry/`. That is how the
> `initial_volume` scale bug was found — see [[playback-and-connect]].

## npm packages ([[package.json]])

| Package | Role |
| --- | --- |
| `@tauri-apps/api` | `invoke` and `listen` bindings |
| `@tauri-apps/plugin-opener` | Frontend side of the opener plugin |
| `svelte` 5 | UI framework (runes) |
| `vite` 6 | Dev server and bundler |
| `@sveltejs/vite-plugin-svelte` 5 | Svelte integration |
| `typescript` 5, `svelte-check` 4 | Type checking |
| `@tauri-apps/cli` 2 | `tauri dev` / `build` / `icon` |

## The Spotify Web API

Base `https://api.spotify.com/v1`, wrapped by [[webapi.rs]]. Every endpoint the
app touches:

| Endpoint | Method | Used by |
| --- | --- | --- |
| `/me` | GET | [[auth.rs]] — profile + Premium check |
| `/me/playlists` | GET | [[library.rs]] |
| `/playlists/{id}/items` | GET | [[library.rs]] |
| `/me/tracks`, `/me/albums` | GET | [[library.rs]] — saved collections |
| `/me/library` | PUT/DELETE | [[library.rs]] — generic URI save/unsave |
| `/me/library/contains` | GET | [[library.rs]] — generic URI saved state |
| `/albums/{id}/tracks` | GET | [[library.rs]] |
| `/artists/{id}`, `/artists/{id}/albums` | GET | [[library.rs]] |
| `/me/top/tracks`, `/me/top/artists` | GET | [[library.rs]] — personalized history alternatives |
| `/me/player/recently-played` | GET | [[library.rs]] — context-aware recent activity |
| `/tracks/{id}`, `/episodes/{id}` | GET | [[player.rs]] — now-playing metadata |
| `/search` | GET | [[search.rs]] |
| `/me/player/devices` | GET | [[connect.rs]] |
| `/me/player` | PUT | [[connect.rs]] — transfer |
| `/me/player/queue` | GET/POST | [[queue.rs]] |

### API quirks the code handles

- **Empty 200 or 204 bodies** on mutations — [[webapi.rs]] maps either to JSON
  null. Query-only generic library mutations send an explicit
  `Content-Length: 0`, required by Spotify's edge.
- **Nulls inside `items`** — search returns `null` entries for unavailable
  results, hence `Vec<Option<T>>` + `flatten` in [[search.rs]]. Playlists
  return null tracks for local files, filtered in [[library.rs]].
- **40-URI cap** on `/me/library` and `/me/library/contains`; Rust chunks both.
- **Simplified track objects** from `/albums/{id}/tracks` carry no nested
  album, so those rows have no cover art.
- **Errors** are `{"error": {"status", "message"}}`, unwrapped in [[webapi.rs]].
- **429 with `Retry-After`** — quota is metered per *client ID* over a rolling
  30-second window, and ours is shared globally by all librespot clients.
  Handled as its own error variant; see [[rate-limiting]].

## LRCLIB

`GET https://lrclib.net/api/get` is the only non-Spotify product API. The Rust
backend supplies exact track, first artist, album, and duration parameters plus
an identifying User-Agent. It uses no key, secret, scraping, or undocumented
endpoint. A 404 becomes an `unavailable` lyrics state; 429 honors
`Retry-After`; synchronized LRC is parsed and sorted locally. See [[lyrics.rs]].

## Platform dependencies

| Dependency | Why | Status |
| --- | --- | --- |
| **WebView2 runtime** | Renders the UI. Tauri does not bundle a browser | Ships with Windows 11 |
| **MSVC build tools** | Linking the Rust binary | Requires VS "Desktop development with C++" |
| **Rust (MSVC toolchain)** | Building | 1.97.1 verified |
| **Node.js 18+** | Frontend build | 24.x verified |

WebView2 is the single largest runtime cost — ~151 MB of the app's ~157 MB
idle private memory. The native side is ~5.9 MB. See [[README.md]].

## Not used

No database, no telemetry, no backend service of our own, no auto-update
server. Product traffic is limited to Spotify and the optional LRCLIB lyrics
lookup described above.

## See also

[[architecture]] · [[build-and-config]] · [[webapi.rs]] · [[Cargo.toml]] ·
[[package.json]] · [[known-limitations]] · [[MOC]]
