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
`productName: "spotify-rust"`, `version: "0.1.0"`,
`identifier: "dev.local.spotify-rust"`.

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
- Only `nsis` is targeted; no MSI, and no macOS/Linux bundles.

## See also

[[build.rs]] · [[default.json]] · [[vite.config.ts]] · [[package.json]] ·
[[app.css]] · [[webapi.rs]] · [[build-and-config]] · [[tauri-config]] · [[MOC]]
