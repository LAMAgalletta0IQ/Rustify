---
tags: [module, backend]
---
# Module — Backend (Rust)

**Path:** `src-tauri/src/` · **40 files** across the root, `audio/`,
`lyrics/`, `spotify/`, and the self-contained `jams/` module.

> Until 2026-08 this counted 14 files and 43 commands. The feature-parity
> and Jam-integration work since then roughly tripled the module: friend
> presence, rich profiles, podcast/music-video/lossless capability
> inspection, telemetry, sleep timer, remote Connect-state projection,
> local relevance ranking, the `spotify/` first-party-services client, and
> the entire `jams/` submodule are all new. `lib.rs` now registers 82
> commands.

The entire backend: audio, authentication, HTTP, state, and the command
surface exposed to the webview.

## Files by role

### Entry and wiring
| File | Role |
| --- | --- |
| [[main.rs]] | OS entry point. 6 lines |
| [[lib.rs]] | Declares modules, builds the Tauri app, registers 82 commands |
| [[state.rs]] | `AppState`, `PlaybackState`, `AuthState`, `TokenStore`, event names |
| [[error.rs]] | `AppError` — the one error type crossing the IPC boundary |

### Core feature modules
| File | Role | Backed by |
| --- | --- | --- |
| [[auth.rs]] | OAuth, token storage, refresher, Premium check | librespot-oauth + Web API |
| [[player.rs]] | librespot session, `Spirc`, event pump | librespot |
| [[connect.rs]] | Device list, transfer | Web API |
| [[library.rs]] | Playlists, albums, liked songs, save/unsave, artist, saved-tracks session cache | Web API |
| [[mod.rs\|lyrics/mod.rs]] | Documented lyrics lookup and synchronized LRC parsing | LRCLIB |
| [[audio-mod]] | CPAL output selection, fallback, presets, and real-time DSP | librespot sink + CPAL |
| [[search.rs]] | Multi-type search (capped at `limit=10`, unlike everything else) | Web API |
| [[queue.rs]] | Queue read/append | Web API |
| [[media_keys.rs]] | Global media-key shortcuts | Tauri plugin |

### First-party features added since the 2026-08 parity pass
| File | Role | Backed by |
| --- | --- | --- |
| [[friends.rs]] | Event-driven friend-presence feed; typed error classification distinguishes a genuine capability refusal (403/404) from a transient failure | SpClient + Dealer pushes |
| [[profiles.rs]] | Rich user profiles and fuzzy user search | SpClient `user-profile-view/v3` |
| [[podcasts.rs]] | Podcast episode resume state, hand-rolled protobuf | Herodotus resumption platform |
| [[music_videos.rs]] | Music-video discovery and its protected-playback boundary | `VIDEO_ASSOCIATIONS` metadata extension |
| [[audio_capabilities.rs]] | Audio-format/storage-resolution capability inspection, deliberately separate from key acquisition | SpClient metadata |
| [[telemetry.rs]] | Privacy-preserving playback-telemetry boundary (Gabo receiver) | SpClient |
| [[sleep_timer.rs]] | Cancellable sleep timer, duration or end-of-track mode | Monotonic time + player events |
| [[remote_state.rs]] | Event-first Spotify Connect state projection — observes the same Dealer message Spirc consumes internally but doesn't expose | Dealer |
| [[relevance.rs]] | Local per-account "quick access" ranking, a JSON file in the app data dir — not a Spotify API | Local disk |
| [[jams_bridge.rs]] | Glue between the self-contained [[jams/mod.rs\|jams/]] module and the real librespot session; owns the one `JamManager`/dealer listener for the controller's life | [[jams/mod.rs\|jams/]] |

### `spotify/` — first-party internal services client
| File | Role |
| --- | --- |
| [[spotify/mod.rs\|mod.rs]] | Entry point: Home, DJ, concerts, credits, artist stats/top tracks, generated-playlist tracklists, fuzzy user search |
| [[spotify/pathfinder.rs\|pathfinder.rs]] | App-wide Pathfinder GraphQL client — **not** the same code as [[jams/pathfinder.rs\|jams/pathfinder.rs]] |
| [[spotify/home.rs\|home.rs]] | Parses the `home` operation: Daily Mix, Discover Weekly, Release Radar, Daylist, and ordinary shelves |
| [[spotify/dj.rs\|dj.rs]] | Resolves the `your_dj` Lexicon context into a track list plus narration metadata; music playback works, narration audio does not (see [[known-limitations]]) |
| [[spotify/concerts.rs\|concerts.rs]] | Parses "Artist On Tour" cards from `queryArtistOverview` |
| [[spotify/credits.rs\|credits.rs]] | Parses `queryTrackCreditsModal` for the credits modal |
| [[spotify/users.rs\|users.rs]] | Fuzzy user search via `searchUsers` |
| [[spotify/artist_extras.rs\|artist_extras.rs]] | Parses listener stats + top tracks from the same `queryArtistOverview` response `concerts.rs` reads |
| [[spotify/playlist_contents.rs\|playlist_contents.rs]] | Pathfinder `fetchPlaylistContents` fallback for the generated-playlist ids (Daily Mix, etc.) that 404 on the public REST tracklist endpoint |

### `jams/` — self-contained Spotify Jam module
| File | Role |
| --- | --- |
| [[jams/mod.rs\|mod.rs]] | Entry point and public surface; depends only on standard crates plus [[jams/token.rs\|token::TokenProvider]] as its one seam into the host app |
| [[jams/session.rs\|session.rs]] | Typed jam model (`JamSession`, `JamMember`, `JamTrack`) and `JamManager`, the public façade |
| [[jams/spclient.rs\|spclient.rs]] | `social-connect/v2/sessions/...` HTTP calls — create/current/join/leave/kick/end/add-track/queue-control. Largest file in the module |
| [[jams/dealer.rs\|dealer.rs]] | WebSocket listener for the real-time push side of a jam — a **separate** dealer connection from librespot's own |
| [[jams/pathfinder.rs\|pathfinder.rs]] | Jam's own Pathfinder client, self-contained, distinct from `spotify/pathfinder.rs` |
| [[jams/client_token.rs\|client_token.rs]] | Mints/manages the internal `client-token` header Pathfinder and SpClient both require |
| [[jams/config.rs\|config.rs]] | Runtime config; hosts are hardcoded, endpoint paths default but are overridable via `jams.toml` |
| [[jams/token.rs\|token.rs]] | The single seam where the module meets the host session — treats a token as an opaque string |
| [[jams/error.rs\|error.rs]] | `JamError`, mapping SpClient/Pathfinder/Dealer failure modes so callers react differently |

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
     ╱  │  │  │  │  │  │  ╲  ╲  ╲  ╲
auth player connect library lyrics search queue
  │      │      ╲    │    ╱      ╱   │      │
  │      │       webapi.rs ◄────┘    │      │
  │      │           │               │      │
friends profiles podcasts music_videos audio_capabilities
telemetry sleep_timer remote_state relevance jams_bridge ──► jams/ (self-contained)
  │      │           │
  └──────┴───────────┴──────► spotify/ (Pathfinder, Home, DJ, concerts, credits, ...)
  │      │           │
  └──────┴───────────┴──────► state.rs, error.rs
```

`state.rs` and `error.rs` are leaves — depended on by everything, depending on
nothing. `commands.rs` is the only module the frontend can reach. `jams/` is
the one subtree that depends on nothing else in the backend (see
[[jams/mod.rs]]'s self-containment note) — `jams_bridge.rs` is the adapter
that wires it to the real session.

## Conventions

- **Errors** — every public fn returns `AppResult<T>`. Commands return
  `AppError`, which serialises as `{kind, message}` so the UI can branch on
  `kind`. See [[error.rs]]. `friends.rs`'s `classify_spclient_error` is the
  reference example of turning a structured `librespot_core` error into the
  right `AppError` variant rather than flattening it to `Other`.
- **Wire vs. summary types** — Web API and Pathfinder modules define private
  `Wire*`/response structs mirroring the source JSON, then convert to flat
  `*Summary` types via `From`/parse functions. Keeps IPC payloads small and
  Svelte components trivial.
- **`serde(rename_all = "camelCase")`** on everything crossing IPC, matching
  [[types.ts]] — watch the direction hazard documented in the top-level
  `CLAUDE.md` for structs that are both deserialised from Spotify (snake_case)
  and serialised to the webview (camelCase).
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
5. [[jams/mod.rs]] — if working on Jam, start there rather than in
   `jams_bridge.rs`; the self-contained module is the one with the real
   logic.

## See also

[[architecture]] · [[entry-points]] · [[data-flow]] · [[state-and-events]] ·
[[frontend-svelte]] · [[tauri-config]] · [[known-limitations]] · [[MOC]]
