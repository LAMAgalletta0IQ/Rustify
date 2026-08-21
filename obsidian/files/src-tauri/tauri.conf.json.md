---
tags: [file, config, build]
---
# `src-tauri/tauri.conf.json`

**Module:** [[tauri-config]] · **Language:** JSON

## Purpose

The Tauri application manifest: identity, window, security policy, how the
frontend is built and located, and what is bundled.

## Key items

### Identity
`productName: "Rustify"`, `version: "0.1.0"`,
`identifier: "dev.local.rustify"`.

The identifier is a reverse-DNS bundle ID. **It determines the app data
directory**, which is where [[auth.rs]] writes `tokens.json` and [[player.rs]]
writes the librespot cache — changing it orphans an existing login.

### `build`
| Key | Value | Meaning |
| --- | --- | --- |
| `beforeDevCommand` | `npm run dev` | Starts Vite for `tauri dev` |
| `devUrl` | `http://localhost:1420` | **Must match the port in [[vite.config.ts]]** |
| `beforeBuildCommand` | `npm run build` | Type-check + bundle before compiling |
| `frontendDist` | `../dist` | Embedded into the binary at compile time |

### `app.windows[0]`
1100×720, minimum 780×520, resizable, `"theme": "Dark"` — matching the
dark-only palette in [[app.css]]. No `label` is set, so the window is `"main"`,
which is what [[default.json]] targets.

`"decorations": false`, `"transparent": true` and
`"windowEffects": { "effects": ["acrylic"] }` give the app a Windows 11
Acrylic backdrop under a self-drawn titlebar — genuine see-through of whatever
is behind the window, not just the wallpaper.

> Previously `"transparent": false` with no window effects, and [[app.css]]
> painted an opaque `#0c0c10` body. The stated reason was that rounding the
> content punched holes at the corners that showed the desktop through. That
> holds for the *content* radius and is still respected — Windows 11 rounds the
> frame itself, so nothing inside the webview is clipped — but it did not
> require an opaque body, and the opaque body is what made the glass read as
> flat grey. Changed 2026-08.

> **Then briefly `micaDark`, replaced by `acrylic` later in 2026-08.** `mica`
> follows the *system* light/dark preference and would have disagreed with
> this app's pinned `"theme": "Dark"`, which is why the Mica period used
> `micaDark` specifically — that reasoning is moot now that no Mica variant is
> configured, but the same pinned-dark logic is why `acrylic` (no light/dark
> variant of its own) needs no equivalent suffix. The switch was because Mica
> only shows the desktop **wallpaper**, never other windows, and read as too
> subtle to be worth the effect; Acrylic shows real content behind the window.
> That surfaced a new problem — see the `.ambient`/`.veil` history in
> [[App.svelte]] and [[app.css]] for how it was addressed, and the
> `windowEffects.effects`-is-a-priority-list and `windowEffects.color`-inert-
> on-Windows-11 gotchas below.

### `app.security.csp`
```
default-src 'self';
img-src 'self' https://i.scdn.co https://mosaic.scdn.co
        https://image-cdn-ak.spotifycdn.com
        https://image-cdn-fa.spotifycdn.com data:;
style-src 'self' 'unsafe-inline';
connect-src 'self' ipc: http://ipc.localhost
```

> **This is the app's main frontend hardening.** `connect-src` permits only
> Tauri IPC, so the webview **cannot make network requests at all** — every
> HTTP call is forced through Rust ([[webapi.rs]]). `img-src` is an allowlist
> of exactly the Spotify CDNs that serve cover art.

`style-src 'unsafe-inline'` is required by Svelte's scoped-style injection.

### `bundle`
`active: true`, `targets: ["nsis"]` (Windows installer only),
`icon: ["icons/icon.ico"]` — see [[icons]].

## Inputs / outputs / side effects

Read by [[build.rs]] at compile time and by `generate_context!()` in
[[lib.rs]]. Not read at runtime — it is baked in.

## Dependencies

**References:** `../dist`, `icons/icon.ico`, npm scripts in [[package.json]]
**Read by:** [[build.rs]], [[lib.rs]]

## Notable logic / gotchas

- **The CSP will silently break new image sources.** Adding a provider whose
  covers come from another host means images fail to load with only a console
  error. Any new CDN must be added to `img-src`.
- **`devUrl` and Vite's port must agree.** [[vite.config.ts]] sets
  `strictPort: true`, so a busy 1420 fails loudly rather than drifting to 1421
  and leaving Tauri pointed at nothing.
- **`frontendDist` is embedded at compile time** — rebuilding `dist/` after
  cargo has run changes nothing. See [[build-and-config]].
- **Changing `identifier` changes the data directory**, effectively logging the
  user out.
- **`transparent`, `windowEffects` and [[app.css]]'s `body` background are one
  setting in three files.** DWM composites Acrylic behind the webview, so any
  opaque paint in the webview hides it and the effect looks like it silently
  failed. Check `body { background: transparent }` before suspecting the
  config.
- **Window effects need an app restart, not a reload.** `tauri dev` rebuilds on
  a `tauri.conf.json` change, but a Vite HMR update alone will not apply a new
  `windowEffects` value.
- **`effects` is a priority list, not a stack.** Tauri (`tauri-utils`)
  documents: "Conflicting effects will apply the first one and ignore the
  rest." Listing `["micaDark", "acrylic"]` silently renders as Mica alone —
  there is no config that layers both. Pick one.
- **`windowEffects.color` does nothing on Windows 11.** It tints
  `Blur`/`Acrylic` only on Windows 10 1903+ per the Tauri docs; on Windows 11
  it is accepted but has no visible effect regardless of value. The only
  levers for tuning Acrylic's tint on this target are `.ambient`/`.veil` in
  [[App.svelte]] and [[app.css]].
- Only `nsis` is targeted; no MSI, and no macOS/Linux bundles.

## See also

[[build.rs]] · [[default.json]] · [[vite.config.ts]] · [[package.json]] ·
[[app.css]] · [[webapi.rs]] · [[build-and-config]] · [[tauri-config]] · [[MOC]]
