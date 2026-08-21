# Rustify

A native, lightweight Spotify client for Windows: Tauri + Rust backend,
Svelte 5 frontend, [librespot](https://github.com/librespot-org/librespot)
0.8 for authentication and audio, and the official Spotify Web API for
metadata.

Personal use. **A Spotify Premium subscription is required** — librespot cannot
play the free, ad-supported tier.

---

## Prerequisites

| Requirement | Status on this machine |
| --- | --- |
| MSVC build tools (VS 2022/2026, "Desktop development with C++") | ✅ present |
| WebView2 runtime | ✅ present (151.x) |
| Node.js 18+ | ✅ present (24.x) |
| Rust (MSVC toolchain) | ✅ present (1.97.1) |

## Setup

```powershell
npm install

# Run in dev (first build compiles librespot — expect several minutes)
npm run tauri dev

# Release build -> src-tauri/target/release/bundle/nsis/
npm run tauri build
```

Icons are already generated from `app-icon.png` (a placeholder). To replace the
artwork, drop in any square PNG and re-run `npm run tauri icon <file>.png`.

On first launch the app itself will ask you to register a free Spotify
Developer app and paste in its Client ID — see "Two logins, first-run Setup"
below for why, and `.env.example` for the environment-variable alternative.

### Pinned dependency

`Cargo.lock` pins **vergen 9.0.6**. vergen 9.1.0 (Jan 2026) ships a
`vergen-lib` 9.1.0 that is incompatible with the `vergen-lib` 0.1.x that
`vergen-gitcl` — and therefore librespot-core 0.8.0's build script — expects.
Letting cargo pick the newest breaks the build with a trait-bound error in
`librespot-core`'s build script. Do not `cargo update` this one without
checking that librespot has caught up.

## Architecture

```
src-tauri/src/
  auth.rs        OAuth via librespot-oauth; token persistence; Premium check;
                 RFC 8628 device authorization
  player.rs      librespot Session + Player + Spirc; PlayerEvent -> Tauri events;
                 decoder-level crossfade
  connect.rs     Connect device list + transfer (Web API)
  remote_state.rs  Dealer-driven Connect cluster/player/queue projection
  library.rs     Playlists, saved albums/tracks, artist albums (Web API)
  search.rs      Search (Web API)
  queue.rs       Local + Connect queue projection (SetQueue events, autoplay)
  sleep_timer.rs Duration/end-of-track sleep timer
  audio/         Equalizer DSP (biquad peaking filters) + output sink
  lyrics/        First-party synced lyrics with LRCLIB fallback
  friends.rs     Dealer-driven friend-presence feed
  profiles.rs    Rich user profiles, follow lists
  podcasts.rs    Herodotus episode resume state
  music_videos.rs  Native Spotify music-video metadata/capability
  audio_capabilities.rs  Per-track format inventory, storage-resolve/v2, Lossless capability
  telemetry.rs   Bounded ledger of genuine local playback (no Gabo impersonation)
  media_keys.rs  Global media-key shortcuts (Play/Pause, Next, Prev)
  state.rs       Central AppState, TokenStore, PlaybackState
  commands.rs    Tauri command layer
  webapi.rs      Thin Web API HTTP client
  jams/          Spotify Jam (social-connect v2) — see README_jams.md
  spotify/       Pathfinder GraphQL client + Home, DJ (Lexicon), concerts,
                 credits, artist stats/top tracks, generated-playlist
                 tracklists, fuzzy user search
src/
  App.svelte              Shell: top nav, error banner, router
  lib/store.svelte.ts     Runes store; subscribes to backend events
  lib/api.ts              Typed wrappers over every Tauri command
  lib/views/               Login, Home, ForYou, Search, Library, Jams, Profile,
                           Setup, AlbumView, ArtistView, PlaylistView, Releases
  lib/components/         TrackList
  lib/features/player/    PlayerBar, SpotifyConnectMenu
  lib/features/lyrics/    Fullscreen Now Playing + synced lyrics
  lib/features/settings/  Settings (quality, output, equalizer, crossfade)
  lib/features/audio/     Output device selection
  lib/ui/                 Shared SelectMenu/menu primitives
```

The module list above is deliberately just names and one line each; read
`obsidian/` (a maintained vault — start at `obsidian/00-index/MOC.md`) for the
*why* behind each one, and `README_jams.md` for Jam specifically.

### Playing a context, not a track

`load_context(context_uri, track_uri?)` loads the *container* (playlist, album,
artist, Liked Songs) and uses `PlayingTrack::Uri` to start at the chosen track,
so playback continues through the rest of it. `load_tracks` handles the cases
with no container — search results and queue entries. Loading a bare track URI
would play one song and stop.

### Media keys

Registered from Rust so they work when the window is unfocused. Registration is
best-effort: media keys are globally exclusive, so if another player already
holds them the app logs a warning and continues rather than failing to start.

### Two logins, first-run Setup

`librespot_oauth` performs **two** loopback-redirect OAuth flows on login, not
one:

- **Streaming** (`http://127.0.0.1:8898/login`) against Spotify's own desktop
  client ID — the one carried by `SessionConfig::default()`. Used only to
  build librespot's `Credentials::with_access_token(...)`, since self-registered
  apps are generally refused the `streaming` scope.
- **Web API** (`http://127.0.0.1:8899/login`) against a Client ID the user
  registers themselves at `developer.spotify.com/dashboard`. Used for every
  library/search/Connect call, so that traffic draws on a private quota
  instead of a quota shared with every other Rustify install.

Because the Web API side needs a Client ID that only the user can supply,
**Setup runs before Login on first launch**: the app checks
`get_login_info` and, if no Client ID is configured yet, shows a guided form
(`src/lib/views/Setup.svelte`) instead of the Login screen. Saving the ID
(`set_client_id`) writes `settings.json` in the app data dir, next to
`tokens.json`, and the app then proceeds straight to Login. A `RUSTIFY_CLIENT_ID`
environment variable still overrides whatever is saved there, for
developers/packagers who want to skip the screen — see `.env.example`.

Access tokens expire in ~1 hour, so `auth::spawn_refresher` renews the Web API
token in the background (5 minutes before expiry) and writes the value into the
shared `TokenStore` that every caller reads. Rotated refresh tokens are
persisted. Only the *Web API* token is on this schedule — librespot's `Session`
maintains its own connection and token provider once connected.

### Why `Spirc` rather than raw `Player`

`Spirc` is what registers the app as a genuine Spotify Connect device. Driving
playback through it (rather than calling `Player` directly) means remote
control from a phone and local control go through the same path, so state
cannot diverge.

### Premium enforcement

After OAuth, `GET /v1/me` is read and the account's own `product` field is
checked before playback starts. A non-Premium account gets an explicit
`PremiumRequired` error on the login screen. This is a plain read of the
account's stated plan — nothing is spoofed or bypassed.

---

## Known limitations (not bugs)

These are upstream gaps, flagged rather than guessed at:

- **Jams** — implemented against Spotify's private `social-connect/v2`
  service (no public Web API exists for this), gated behind first-party
  credentials librespot's own session already holds. See `README_jams.md`
  for the real endpoints, the bearer-token gotchas, and current status.
- **DJ narration audio** — the music track playback and Lexicon/dynamic-context
  resolution work; narration (the spoken commentary between tracks) resolves
  a real, valid signed playback URL from Spotify's TTS endpoint but nothing
  plays it yet. That would need a second audio pipeline running alongside
  librespot's own Sink, which needs live device testing to get right.
- **Spotify-native Lossless** — FLAC decoding, per-track format inventory and
  storage-resolve/v2 probing are implemented and report an honest capability
  state; actual protected-stream playback needs a PlayPlay key this project
  will not extract or bypass. Blocked on that, not on missing plumbing.
- **Gabo playback telemetry** — genuine local playback is tracked in a bounded
  ledger, but it is never uploaded to Spotify's Gabo endpoint: doing so
  legitimately would require impersonating a first-party client, which this
  project treats as a hard line, not a missing feature.
- **Blends** — read/play only. An existing Blend is an ordinary playlist and
  appears and plays like one. *Creating* a Blend or inviting a participant uses
  a private, undocumented endpoint and is not implemented.
- **Queue reordering** — the Web API supports appending to the queue
  (`POST /me/player/queue`) but offers no reorder or remove operation.
- **Artist top tracks** are read from Pathfinder when available (real
  Spotify-ranked tracks with no fabricated "Popular" chart), falling back to
  an ad-hoc sample built from recent albums if that field isn't there; either
  way they play as a track list rather than a browsable Spotify context.
- **Album track rows carry no cover art** — `/albums/{id}/tracks` returns
  simplified track objects with no nested album. The header shows the art.
- **`GET /me/player/queue`** reflects the *active* device's queue. Accurate
  while this app is the active device, which is the normal case.
- **`isActiveDevice`** is inferred from player events rather than read
  directly; librespot exposes no explicit "am I active" flag.

## Measured footprint

Taken on this machine, 2026-08-17, release build — **predates the Jam/DJ/
Home/lyrics/friends/profile/telemetry/audio-capability work below**, all of
which add dependencies and background tasks. Re-measure before quoting this
number for the current build.

| Idle | Rustify | Official client |
| --- | --- | --- |
| Processes | 7 (1 Rust + 6 WebView2) | 7 |
| Private (commit) | **157 MB** | 1442 MB |
| Working set | 347 MB | 818 MB |

**Not apples-to-apples**: Rustify was on the login screen, the official
client was logged in with a home feed rendered. Redo this with both logged in
and playing before quoting a ratio.

The meaningful breakdown: the Rust process is **5.9 MB private / 27.5 MB
working set**. WebView2 accounts for ~151 MB of the 157 MB. The native side is
effectively free; the webview is the floor for any HTML-based UI, so further
memory work means shrinking or replacing the webview, not optimising Rust.

## Manual test checklist

1. `npm run tauri dev` against a fresh app data dir (no `settings.json`) —
   window opens on the **Setup** screen, not Login. The redirect URI shown
   must be `http://127.0.0.1:8899/login`. Saving an empty ID must be blocked;
   saving a real one must move straight to Login and survive a restart
   (`settings.json` persists it, distinct from `tokens.json`).
2. **Login** — browser opens, approval returns to the app; avatar and display
   name appear. A non-Premium account must show the "Premium required" message.
3. **Playback** — open a playlist, click a track, confirm audio and that the
   player bar shows title/artist/art and a moving progress bar.
4. **Context continuation** — let that track run to the end (or press next):
   playback must continue to the *following track in the playlist*, not stop.
   Same for an album, Liked Songs, and a track picked from search results.
5. **Transport** — pause/resume, next/prev, seek, volume, shuffle, repeat.
6. **Connect (inbound)** — open Spotify on a phone, open the device list, and
   confirm `<COMPUTERNAME> (Rustify)` is listed and selectable; controlling
   from the phone must update this app's UI.
7. **Connect (outbound)** — use the ▣ button to move playback to another
   device, then "Play here" to pull it back.
8. **Library / search / queue** — playlists, saved albums, liked songs load;
   search returns all four result types; ＋ appends to the queue.
9. **Save/unsave** — the ♡ on a track row fills in and the change survives a
   refresh; same for the album header heart.
10. **Drill-down** — a search hit on an album or artist opens a detail view
    (not instant playback); artist → album navigation works.
11. **Pagination** — "Load more" appears for libraries over 50 items and the
    button disappears on the last page.
12. **Media keys** — Play/Pause, Next and Prev on the keyboard work with the
    window minimised. If another player holds them, check the log for the
    registration warning rather than assuming a bug.
13. **Token expiry** — leave the app running over an hour, then load a
    playlist. It must work; a 401 means the refresher is broken.
14. **Restart** — relaunching skips the browser login (stored refresh token).
15. **Resource usage** — compare idle/active RAM and CPU against the official
    client in Task Manager.

The features below were added after this checklist was written and are not
yet folded into it as numbered steps; each has its own status/caveats
documented where it lives (`README_jams.md` for Jam; `obsidian/` for the
rest). At minimum, before relying on a build, confirm each of these actually
does something rather than just render:

16. **Home / Made For You** — Daily Mix, Discover Weekly, Release Radar and
    similar cards load with real artwork, open, and play.
17. **DJ** — starting it plays real tracks; check the log for narration
    (expected to resolve but not play — see Known limitations).
18. **Jam** — create one, copy the invite, have a second account join it,
    confirm both sides see member/queue updates live.
19. **Lyrics** — open fullscreen on a track with synced lyrics; the active
    line follows playback and manual scroll suspends auto-follow until you
    return to it.
20. **Friend Activity** — the sidebar rail shows real presence for accounts
    that have visible friend activity, not a static "unavailable" message.
21. **Sleep timer / crossfade / equalizer** — set each from Settings/Now
    Playing and confirm an audible effect, not just a UI state change.
