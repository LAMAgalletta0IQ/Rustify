# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```powershell
npm install
npm run tauri dev            # dev app; first build compiles librespot (several minutes)
npm run tauri build          # release -> src-tauri/target/release/bundle/nsis/
npm run check                # svelte-check only
npm run build                # svelte-check && vite build (frontend only)
cd src-tauri; cargo check --no-default-features   # fast Rust type-check
```

`--no-default-features` matches what `tauri dev` passes; omitting it compiles a
different feature set than the app actually runs.

```powershell
cd src-tauri; cargo test --no-default-features --lib   # 125 unit tests
npm test                                                # vitest run, store.svelte.ts
cd src-tauri; cargo fmt --check
cd src-tauri; cargo clippy --no-default-features -- -D warnings
```

**There is a Rust unit-test suite (125 tests) and a small Vitest suite for
`store.svelte.ts`** (10 tests, mocking `@tauri-apps/api/core`/`event`) — no
component tests, no E2E.

> Until 2026-08 this file said there was no test suite at all, then later that
> there was a Rust suite but no frontend one. Both were wrong when written and
> are wrong now: `cargo test --lib` has covered audio DSP, telemetry,
> Pathfinder hash extraction, jam payload parsing, settings migration, token
> persistence merge behaviour, grant-rejection classification, position
> anchoring and session-generation monotonicity for some time; `npm test`
> covers `store.svelte.ts`'s error-handling/auth-clearing, position ticker,
> stale-lyrics-request cancellation, and optimistic-toggle rollback.

What has not changed is the conclusion those lines were drawing. The Rust tests
are all pure-function tests over fixed payloads, and the Vitest suite mocks
every Tauri IPC boundary rather than driving a real webview: **none of them
start Tauri, librespot, or a real webview**, so a green run says nothing about
runtime behaviour. The verification loop is `cargo check --no-default-features`
+ `cargo test --lib` + `npm run check` + `npm test`, and `README.md` has a
15-step manual checklist covering login, playback, Connect, and token expiry —
that checklist is the E2E layer this project has, not a stopgap for automation
that is coming. Most real bugs in this codebase have only ever been found by
running the app and reading the log — say which of these you actually ran, and
do not imply the others.

Useful when diagnosing: `$env:RUST_LOG="debug,librespot=warn"` before
`npm run tauri dev`. `RUST_LOG` can also live in `.env`.

## Configuration

`.env` at the project root, loaded by `load_dotenv()` in `lib.rs` **before**
`env_logger::init()` so `RUST_LOG` applies. `.env.example` is the committed
template; `.env` is gitignored.

| Variable | Effect |
| --- | --- |
| `RUSTIFY_CLIENT_ID` | Client ID for Web API traffic. Unset → shares librespot's globally-pooled quota and 429s spuriously |
| `RUST_LOG` | Log filter |

Startup logs which mode is active — check this first when debugging quota or
scope problems:

```
[INFO rustify_lib] loaded environment from D:\projects\Rustify\.env
[INFO rustify_lib] web api client id: private (from environment)
```

## Architecture

Tauri 2 shell: Rust backend (`src-tauri/src/`) + Svelte 5 webview (`src/`),
talking over Tauri commands and events. librespot 0.8 does auth and audio; the
Spotify Web API does metadata. `README.md` lists the files; below is what you
cannot infer from them.

### Two credentials, not one

This is the least obvious thing in the codebase. There are **two OAuth logins**
against **two client IDs** (`auth.rs`):

- **Streaming** — Spotify's desktop client ID from `SessionConfig::default()`.
  Hardcoded, because self-registered apps are generally refused the `streaming`
  scope. Redirect `127.0.0.1:8898`, falling back to an ephemeral port.
- **Web API** — the user's own Client ID. Redirect `127.0.0.1:8899`, **fixed**:
  a self-registered app must declare its redirect URI exactly in Spotify's
  dashboard, so this one cannot vary.

With both configured the browser opens **twice** on login.

> Until 2026-08 an unconfigured Web API ID silently fell back to a client ID
> baked into the binary, so the app "just worked" without setup. That meant
> every user who skipped configuration shared *the developer's* quota — which
> does not scale past one person running the app, and defeats the entire
> point of splitting the two logins. There is no fallback now: on first
> launch, before Login is even shown, the UI blocks on a Setup screen
> (`Setup.svelte`) that walks the user through registering their own Spotify
> app and saves the Client ID to `settings.json` in the app data dir via the
> `set_client_id` command. `auth::webapi_client_id(data_dir)` resolves it from,
> in order: the `RUSTIFY_CLIENT_ID` env var (a packager/dev override, not the
> primary path anymore), then `settings.json`, then errors — which
> `restore_session`/`get_login_info` interpret as "Setup still needed" rather
> than a real failure.

Consequences that bite:

- `STREAMING_SCOPES` is the **full union**, not just `streaming`, because that
  token is the fallback. Narrowing it makes the fallback silently
  under-privileged — search keeps working (needs no scope) while library and
  player calls return a bare 403.
- Spotify **rotates refresh tokens on use**. If the streaming refresh succeeds
  and a later step fails, the stored token is already dead and its replacement
  exists only in memory. `restore_login` must degrade rather than return `Err`.
- Only `invalid_grant`/`invalid_client` may delete `tokens.json`
  (`auth::is_grant_rejected`). Clearing on any failure logs the user out on a
  startup network blip.
- An empty refresh token must be stored as **absent**, never `Some("")` —
  Spotify answers `invalid_request: refresh_token must be supplied`.
- `save_stored_tokens` **merges**: a session with no Web API refresh token
  keeps whatever is already on disk. Writing `None` through erased the private
  credential after any degraded restore, and the next launch then fell to the
  shared quota and 429'd again — a self-sustaining loop out of three
  individually-correct behaviours. `clear_stored_tokens` deletes the file, so
  intentional clearing still works.
- Spotify returns `refresh_token` **only when it rotates one**; an omitted
  field means "keep yours", but deserialises to `""`. `restore_login` writes
  the old value back over an empty one.
- `establish` logs `persisting tokens: streaming=…, web api=… (split=…)`.
  Check it before theorising about quota — a token that never reaches disk is
  otherwise invisible until the next launch.

- `ugc-image-upload` was added to both scope lists in 2026-08, for playlist
  cover upload. A stored refresh token is re-exchanged for **the scope set it
  was granted**, not the current list, so tokens from before that lack it and
  only an interactive re-login adds it. `library::update_playlist_image`
  rewrites the resulting 403 to say so.
- **`tokens.json` is DPAPI-encrypted, bound to the current Windows user.**
  `save_stored_tokens`/`load_stored_tokens` wrap a private `auth::dpapi`
  submodule (`CryptProtectData`/`CryptUnprotectData`, no
  `CRYPTPROTECT_LOCAL_MACHINE` flag) around the same JSON shape as before —
  the merge-preserve behaviour above is unchanged, it just now reads and
  writes through DPAPI instead of `std::fs` directly. A pre-encryption
  plaintext file is migrated in place the first time `load_stored_tokens`
  reads it (parses as JSON, then immediately re-saves encrypted), so an
  existing install upgrades silently with **no forced re-login**. Anything
  `CryptUnprotectData` rejects — a corrupted file, one copied from another
  machine, one from another Windows account — and that also fails to parse as
  plaintext JSON degrades to "no stored tokens" (`None`), never a panic. If
  `CryptProtectData` itself fails on save (undocumented for an interactive
  session, but not assumed impossible), `write_protected` falls back to
  plaintext rather than losing the session, and logs a warning.

`establish()` in `commands/session.rs` is where the two tokens diverge: the Web
API token goes to `/me` and `TokenStore`; the *streaming* token goes to
librespot.

The Client ID is manageable after setup: `get_login_info` returns the ID itself
plus `client_id_from_env`, and Settings edits or clears it (`clear_client_id`).
The env var wins in `auth::webapi_client_id`, so the field is disabled while it
is set rather than silently no-opping.

### Spirc, and registering ≠ activating

Everything playback-related goes through `Spirc`, never `Player` directly, so
local clicks and phone commands take the same path and cannot diverge.

`start_session` deliberately does **not** call `spirc.activate()`. Activating
claims active-device status, which by protocol pauses playback on the user's
other devices — so calling it at login made merely *opening the app* a takeover.
Activation belongs in `load_context`/`load_tracks` (which check
`is_active_device` first) and `activate_this_device`.

Corollary: transport commands drive **this app's player only**. While another
device is active the UI mirrors its state but the buttons do not command it.

### PlaybackState has two writers

- `spawn_event_pump` — librespot `PlayerEvent`s, push-based, describes only
  audio *this app* produces.
- `spawn_remote_poller` — `GET /me/player` every 5 s, **only while not
  `is_active_device`**. Without it a passive device shows "Nothing playing".

This is the app's only recurring network timer, and it is gated deliberately.
Read `obsidian/concepts/rate-limiting.md` before adding another.

**Position is anchored, not assigned.** librespot reports a position on only
some events; `VolumeChanged`/`ShuffleChanged`/`RepeatChanged` carry none. Since
the whole snapshot ships on every event, a bare assignment sent a stale 0 and
reset the UI clock mid-track. Use `set_position()` to write and
`refresh_position()` before any snapshot leaves the backend.

Both background tasks live on `SpotifySession` and must be `.abort()`ed on
logout and session replacement — dropping a `JoinHandle` **detaches** rather
than cancels, which once left two refreshers hammering the token endpoint.

### A closed player channel is recoverable, and must not log the user out

If librespot's `Player` dies, the `PlayerEvent` sender drops, `rx.recv()`
returns `None`, and every later transport call fails with
`Internal error { channel closed }`. `spawn_event_pump` used to just exit,
leaving the dead `Spirc` installed — the app then needed a restart.

It now calls `recover_closed_session`, which rebuilds the session from the
stored refresh tokens on a 2/6/15/45 s backoff. Two things make this safe:

- **`AppState::session_generation`.** Each pump is tagged with the generation
  it was spawned for. `establish` claims a new one before building anything and
  `logout` retires one *before* shutting the Spirc down — because that shutdown
  closes the channel too. Without the tag, every ordinary logout would trip the
  watchdog into logging the user straight back in.
- **It never clears tokens.** `rebuild_session` reuses `restore_login` but does
  not delete on failure and does not return a logged-out `AuthState`. A dropped
  socket must not become a forced re-login.

### Adding a command needs three edits

`#[tauri::command]` in the right `commands/*.rs` domain file (`session.rs`,
`playback.rs`, `library.rs`, `discovery.rs`, or `jams.rs` — see
`commands/mod.rs`'s module doc for which is which; `commands/mod.rs` re-exports
all of them, so `lib.rs` and callers still just say `commands::whatever`), the
identifier in `generate_handler![]` in `lib.rs`, and a wrapper in
`src/lib/api.ts`. Missing the second or third used to fail only at **runtime**
("command not found"), never at compile time.

That gap is now caught by a test: `commands::registration_parity_tests` in
`commands/mod.rs` reads `lib.rs` and `src/lib/api.ts` as text and asserts every
`generate_handler!` entry has a matching `invoke("...")` call site in `api.ts`
and vice versa, plus a third test asserting `state::events`' string values
match `api.ts`'s `EVENT_*` constants. Deliberately text-based rather than a
build-time macro — cheap, and it runs with every `cargo test --lib`. Its
limits: it cannot see through a command name built from a variable (neither
side does that today) and it does not check argument shapes, only that both
sides agree on which command/event names exist.

### serde direction hazard

`#[serde(rename_all = "camelCase")]` applies to **both** directions. Structs
that are deserialised from Spotify (snake_case) *and* serialised to the webview
(camelCase) need `rename_all(serialize = "camelCase")`. Getting this wrong
produced `missing field 'isActive'` on every device list, which surfaced as an
innocent-looking "No devices found".

### Web API quirks encoded in constants

`/search` caps `limit` at **10** (`search::MAX_SEARCH_LIMIT`) — far below the 50
other endpoints accept, and exceeding it is a hard `400 Invalid limit`. The 50s
in `library.rs` are correct for those endpoints.

**`/me/following` is the only library endpoint that pages by cursor**, not
offset: it takes `after=<last artist id>` and nests its page under an `artists`
key rather than returning the page object at the top level. Everything else in
`library.rs` is `limit`/`offset`, so `followed_artists` returns an `ArtistPage`
carrying the next cursor, and callers must hand that back instead of counting
items they already hold.

**`/playlists/{id}/items` keys its rows `item`, not `track`**, so
`PlaylistItem.track` needs `#[serde(alias = "item")]`. Because the field is
`Option`, the wrong key is not a parse error — every row deserialises to `None`
and `filter_map` drops it, so the playlist opens to "Nothing here." with a 200
in the log and no warning anywhere. Confirm a wire shape with `fields=` before
assuming the field name.

**`/v1/audio-features` is gone for this app.** Spotify closed it to apps in
Development Mode, which is what a self-registered Client ID is — so
danceability/energy/valence are unobtainable by any request this app can make,
and scraping them is out. `dna.rs` builds its profile from `genres` +
`popularity` on `/me/top/artists` and album `release_date` on
`/me/top/tracks` instead, and labels every axis with what it measured.

`lastfm.rs` optionally enriches the `genres` half (Spotify leaves it empty for
many artists) when the user saves an API key in Settings. **It is opt-in and
must stay that way** — with no key it is never called and DNA is unchanged, and
`tag_source` reports what was *used* so a rejected key never reads as enriched.
It also needs its `NON_GENRE_TAGS` blocklist: `seen live` and `female
vocalists` are among Last.fm's most-applied tags and swamp real genres
otherwise. Match whole tags, never substrings — `uk`/`love` would eat `uk
garage`/`lovers rock`.

**Lexicon (DJ) 403s for most accounts.** `start_dj` and `get_dj_status` catch
`LexiconUnavailable` and fall back to the public DJ playlist, tagging the
session `reason: "lexicon-unavailable-fallback"` (mirrored as a constant in
`Home.svelte`) with every dynamic flag false. Do not "fix" that by making the
fallback look like a resolved session — `spawn_dj_refill` keys off those flags.

**`PUT /playlists/{id}/images` is not JSON.** It wants raw base64 JPEG with
`Content-Type: image/jpeg` (hence `WebApi::put_raw`), capped at 256 KB of
*base64*. `src/lib/images.ts` re-encodes in the webview so the backend needs no
image codec.

`webapi.rs` logs method, URL, status and Spotify's own message on every failed
request. Read that line before theorising; it is how the search cap was found.
Note that Spotify answers a malformed `Authorization` header with **400**, not
401.

## Generated TypeScript types (ts-rs)

`src/lib/generated/*.ts` — `PlaybackState`, `AuthState`, `TrackInfo`, `Device`,
`ConnectionStatus`, `StreamQuality` — are generated from their Rust
`#[derive(TS)]` definitions (`state.rs`, `connect.rs`, `audio/mod.rs`) instead
of hand-mirrored in `src/lib/types.ts`. `types.ts` re-exports them
(`export type { PlaybackState } from "./generated/PlaybackState";`) so every
existing `import type { PlaybackState } from "./types"` elsewhere in the
frontend kept working with zero call-site changes.

- `npm run gen:types` regenerates them (`cargo test --features ts-rs-export
  --lib export` under the hood).
- `npm run check:types` regenerates into a scratch state and fails
  (`git diff --exit-code`) if the committed files would change — the drift
  check for "someone edited the Rust struct and forgot to regenerate."
- `ts-rs` is an **optional** regular dependency behind the `ts-rs-export`
  Cargo feature (same pattern as `mcp-bridge`) — `derive(TS)` sits on real
  struct definitions, not test-only code, so it cannot be a dev-dependency,
  but it is still absent from the default/release build
  (`#[cfg_attr(feature = "ts-rs-export", derive(TS))]` / `#[cfg(feature =
  "ts-rs-export")] use ts_rs::TS;`).
- `export_to` paths are **not** relative to `CARGO_MANIFEST_DIR` — ts-rs joins
  them onto its own default export root (`./bindings`, relative to the
  process's CWD at test time), so `"../../src/lib/generated/"` was needed to
  land in the frontend tree at all; a single `"../src/lib/generated/"`
  silently lands inside `src-tauri/src/lib/generated/` instead. If bindings
  ever start appearing in the wrong place after a ts-rs upgrade, this is
  the first thing to check.
- **Only started with the core IPC-boundary types** (per the review that
  prompted this). `src/lib/types.ts` still hand-mirrors everything else
  (`AppErrorPayload`, `LoginInfo`, library/search/queue shapes, ...) —
  `AppErrorPayload` in particular is not a good ts-rs candidate as-is: its
  wire shape (`{ kind, message, retryAfter }`) comes from `AppError`'s
  hand-written `impl Serialize`, not from the enum's natural derive shape, so
  deriving `TS` directly on the `AppError` enum would generate the wrong
  type. Extend coverage type-by-type, not all at once.
- On Windows, generating requires an MSVC dev environment on `PATH` (`rc.exe`
  for `tauri-build`'s icon embedding) — a plain Git Bash shell without VS
  Build Tools loaded will fail with `Are you sure you have RC.EXE in your
  $PATH`; run it from a Developer PowerShell/cmd instead, same as
  `npm run tauri build`.

## Documentation vault

`obsidian/` is a maintained Obsidian vault: `concepts/` for cross-cutting
design, `files/` mirroring the source tree one note per file, `00-index/MOC.md`
as the entry point. It documents *why* and records failure modes, not just
rules.

**Keep it current when changing behaviour it describes.** The concept notes on
auth, rate limiting, playback/Connect and state are the ones that go stale
fastest. Where guidance has reversed, the notes preserve the old reasoning in a
blockquote with what changed — follow that pattern rather than deleting history.

### The window is undecorated

`tauri.conf.json` sets `decorations: false`, `transparent: true` and
`windowEffects: { effects: ["acrylic"] }` so the app can draw its own rounded,
frosted shell over a Windows 11 Acrylic backdrop. Consequences:

- **The three must move together.** DWM paints Acrylic *behind* the webview,
  so the webview has to be see-through for any of it to reach the screen:
  `body` is `background: transparent`, and one opaque paint anywhere in the
  stack — `#0c0c10` on `body`, a solid base colour under `.ambient` — hides
  the backdrop completely and the app is a flat dark rectangle again.

  > Until 2026-08 the window was deliberately opaque (`transparent: false`,
  > `body { background: #0c0c10 }`) on the grounds that rounding the content
  > punched holes at the corners showing the desktop through. That reasoning
  > still holds for *content* radius — Windows 11 rounds the frame itself, so
  > the content must stay square — but it never required an opaque body.

- **`backdrop-filter` cannot blur Acrylic.** It only blurs what the webview
  painted, and the backdrop is composited outside it. Panels over bare Acrylic
  read as tint plus edge, never as an extra blur, which is why `app.css`
  carries an explicit three-step elevation scale (`--glass` → `--glass-raised`
  → `--glass-strong`) and a lit top edge (`--edge`) instead of leaning on blur
  radius for depth. Raising `--blur` to compensate does nothing.

- `.ambient`/`.veil` in `App.svelte` sit *on top of* Acrylic rather than being
  the background, so both are kept well under full opacity — lighter than they
  were under Mica, since Acrylic already does some of the legibility work
  itself. Every point of veil is a point of backdrop removed.

  > Until 2026-08 this used `micaDark` instead. Mica only shows the
  > **wallpaper**, never the windows behind, and reads as too subtle even at
  > higher `.ambient`/`.veil` opacity. Switching to `acrylic` gives genuine
  > see-through (real windows behind, not just wallpaper) but exposes a
  > problem Mica never had: a solid, alpha-blended tint over arbitrarily
  > saturated real content (not just a wallpaper you can co-design against)
  > reads as a muddy clash rather than a tint. Fixed by lowering `.ambient`'s
  > opacity, blurring its gradient edges (`filter: blur(70px)`) so there is no
  > hard edge left to collide with, and desaturating it slightly
  > (`saturate(0.85)`) — not by darkening the veil, which just hides the
  > backdrop instead of fixing the collision.
  >
  > **`windowEffects.effects` is a priority list, not a stack** — Tauri
  > silently applies only the first supported entry and drops the rest
  > (`tauri-utils`: "Conflicting effects will apply the first one and ignore
  > the rest"). `["micaDark", "acrylic"]` therefore renders as Mica alone;
  > there is no way to layer both. Also note `windowEffects.color` (a tint for
  > `Acrylic`/`Blur`) has no effect on Windows 11 regardless of value — only
  > Windows 10 1903+ honours it — so Acrylic's own tint cannot be adjusted
  > from config on this target and `.ambient`/`.veil` are the only knobs.

- **A drag region still works while fullscreen.** The fullscreen lyrics view
  binds `data-tauri-drag-region={fullscreen ? undefined : true}` — otherwise
  the window could be dragged out of fullscreen by its header. Use `undefined`,
  never `false`: Svelte omits the attribute for `undefined`, while `false` on a
  non-boolean attribute still renders `data-tauri-drag-region="false"`, which
  Tauri matches on presence and keeps honouring.
- **The title bar in `App.svelte` is the only way to move the window.** Any
  region that should drag needs `data-tauri-drag-region`, and the login screen
  carries its own strip — without it the window is immovable before sign-in.
- Minimize/maximize/close are ordinary buttons calling `getCurrentWindow()`,
  which is why `capabilities/default.json` grants `core:window:allow-minimize`,
  `allow-toggle-maximize`, `allow-is-maximized` and `allow-close`.
- Undecorated Windows windows lose Snap Layouts on the maximize button and the
  native drop shadow. That is the trade for the rounded glass edge.

## Repository gotchas

- **`Cargo.lock` pins vergen 9.0.6.** 9.1.0 breaks librespot-core 0.8.0's build
  script with a trait-bound error. Do not `cargo update` it blindly.
- **Cargo bakes absolute paths into `target/`.** Moving or renaming the project
  directory produces `failed to read plugin permissions: ... The system cannot
  find the path specified`. Fix with `cargo clean` (or `cargo clean -p` for the
  affected packages), not by editing anything.
- **The Tauri `identifier` determines the app data dir**, so changing it orphans
  `tokens.json` and forces a fresh login.
- **A Spotify Client ID is not a secret** — it travels in the clear in every
  OAuth redirect and this flow is PKCE with no client secret. `.env` is
  gitignored for tidiness, not security.
- **Password login is impossible.** `Credentials::with_password` still exists in
  librespot 0.8 and compiles, but Spotify disabled it server-side in July 2024.
  OAuth is the only route.
- **Version bumps touch three files.** `package.json`, `src-tauri/Cargo.toml`,
  and `src-tauri/tauri.conf.json` all carry an independent `version` field and
  none of them derive from another — update all three together or the build,
  the User-Agent (`webapi.rs`'s `rustify/{CARGO_PKG_VERSION}`), and the
  installer disagree on what shipped.
- **`tauri-plugin-mcp-bridge` is opt-in via the `mcp-bridge` Cargo feature**,
  off by default, so it is absent from the default/release dependency graph
  (`cargo tree --no-default-features` shows no `tauri-plugin-mcp-bridge`), not
  merely inactive. `lib.rs` only registers the plugin under
  `cfg(all(debug_assertions, feature = "mcp-bridge"))`. To use it locally:
  `cp src-tauri/capabilities/mcp-bridge.json.example src-tauri/capabilities/mcp-bridge.json`,
  then `cargo check --no-default-features --features mcp-bridge` (or the
  equivalent `tauri dev` invocation). Tauri capability files are static JSON
  validated against the permission schema of whatever plugins are actually
  compiled in — leaving `mcp-bridge.json` in place for a build *without* the
  feature fails with `Permission mcp-bridge:default not found`, which is why
  the real file is gitignored and only the `.example` template is committed.
  Delete `capabilities/mcp-bridge.json` again before building without the
  feature.
