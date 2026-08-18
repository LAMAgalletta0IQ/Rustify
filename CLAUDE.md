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

**There is no test suite** — no `#[test]`, no vitest. Do not claim tests pass.
The verification loop is `cargo check --no-default-features` + `npm run check`,
and neither proves runtime behaviour. `README.md` has a 15-step manual checklist
covering login, playback, Connect, and token expiry; most real bugs in this
codebase have only ever been found by running the app and reading the log.

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
- **Web API** — `RUSTIFY_CLIENT_ID`. Redirect `127.0.0.1:8899`, **fixed**: a
  self-registered app must declare its redirect URI exactly in Spotify's
  dashboard, so this one cannot vary.

With both configured the browser opens **twice** on login. Without a private ID,
`SessionTokens::shared()` points both roles at the streaming token.

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

`establish()` in `commands.rs` is where the two tokens diverge: the Web API
token goes to `/me` and `TokenStore`; the *streaming* token goes to librespot.

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

### Adding a command needs three edits

`#[tauri::command]` in `commands.rs`, the identifier in `generate_handler![]` in
`lib.rs`, and a wrapper in `src/lib/api.ts`. Missing the second or third fails
at **runtime** ("command not found"), never at compile time. Event name
constants are likewise duplicated between `state::events` and `api.ts` with
nothing enforcing the match.

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

`webapi.rs` logs method, URL, status and Spotify's own message on every failed
request. Read that line before theorising; it is how the search cap was found.
Note that Spotify answers a malformed `Authorization` header with **400**, not
401.

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
