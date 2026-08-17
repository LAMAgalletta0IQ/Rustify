# spotify-rust

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
  auth.rs      OAuth via librespot-oauth; token persistence; Premium check
  player.rs    librespot Session + Player + Spirc; PlayerEvent -> Tauri events
  connect.rs   Connect device list + transfer (Web API)
  library.rs   Playlists, saved albums/tracks (Web API)
  search.rs    Search (Web API)
  queue.rs     Queue read/append (Web API)
  media_keys.rs Global media-key shortcuts (Play/Pause, Next, Prev)
  state.rs     Central AppState, TokenStore, PlaybackState
  commands.rs  Tauri command layer
  webapi.rs    Thin Web API HTTP client
src/
  App.svelte           Shell: top nav, error banner, router
  lib/store.svelte.ts  Runes store; subscribes to backend events
  lib/api.ts           Typed wrappers over every Tauri command
  lib/views/           Login, Home, Search, NowPlaying, AlbumView, ArtistView
  lib/components/      PlayerBar, DevicePicker, TrackList
```

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

### One login, two consumers

`librespot_oauth` performs a loopback-redirect OAuth flow
(`http://127.0.0.1:8898/login`) against Spotify's own desktop client ID — the
one carried by `SessionConfig::default()`. The resulting access token is used
**both** as librespot's `Credentials::with_access_token(...)` and as the Web
API bearer token, so there is a single credential and a single refresh path.
The refresh token is persisted to the app data dir and reused on next launch.

If a scope is ever refused by that client ID, the fallback is to register your
own Spotify Developer app and substitute its client ID in `auth.rs`.

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

- **Jams** — no support. Spotify exposes no public Web API for Jams, and
  librespot implements nothing for them. Omitted entirely.
- **Blends** — read/play only. An existing Blend is an ordinary playlist and
  appears and plays like one. *Creating* a Blend or inviting a participant uses
  a private, undocumented endpoint and is not implemented.
- **Lyrics** — no public Web API endpoint exists. The full now-playing view
  leaves layout space for a future provider.
- **Queue reordering** — the Web API supports appending to the queue
  (`POST /me/player/queue`) but offers no reorder or remove operation.
- **Artist top tracks have no container context**, so they play as an ad-hoc
  track list rather than a browsable Spotify context.
- **Album track rows carry no cover art** — `/albums/{id}/tracks` returns
  simplified track objects with no nested album. The header shows the art.
- **`GET /me/player/queue`** reflects the *active* device's queue. Accurate
  while this app is the active device, which is the normal case.
- **`isActiveDevice`** is inferred from player events rather than read
  directly; librespot exposes no explicit "am I active" flag.

## Measured footprint

Taken on this machine, 2026-08-17, release build:

| Idle | spotify-rust | Official client |
| --- | --- | --- |
| Processes | 7 (1 Rust + 6 WebView2) | 7 |
| Private (commit) | **157 MB** | 1442 MB |
| Working set | 347 MB | 818 MB |

**Not apples-to-apples**: spotify-rust was on the login screen, the official
client was logged in with a home feed rendered. Redo this with both logged in
and playing before quoting a ratio.

The meaningful breakdown: the Rust process is **5.9 MB private / 27.5 MB
working set**. WebView2 accounts for ~151 MB of the 157 MB. The native side is
effectively free; the webview is the floor for any HTML-based UI, so further
memory work means shrinking or replacing the webview, not optimising Rust.

## Manual test checklist

1. `npm run tauri dev` — window opens on the login screen.
2. **Login** — browser opens, approval returns to the app; avatar and display
   name appear. A non-Premium account must show the "Premium required" message.
3. **Playback** — open a playlist, click a track, confirm audio and that the
   player bar shows title/artist/art and a moving progress bar.
4. **Context continuation** — let that track run to the end (or press next):
   playback must continue to the *following track in the playlist*, not stop.
   Same for an album, Liked Songs, and a track picked from search results.
5. **Transport** — pause/resume, next/prev, seek, volume, shuffle, repeat.
6. **Connect (inbound)** — open Spotify on a phone, open the device list, and
   confirm `<COMPUTERNAME> (spotify-rust)` is listed and selectable; controlling
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
