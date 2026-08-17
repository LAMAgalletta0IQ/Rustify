---
tags: [concept, build, config]
---
# Build and configuration

## Toolchain

| Requirement | Verified | Notes |
| --- | --- | --- |
| Rust (MSVC) | 1.97.1 | `x86_64-pc-windows-msvc` |
| MSVC build tools | VS 2026 Community | "Desktop development with C++" |
| Node.js | 24.18.0 | 18+ works |
| WebView2 runtime | 151.x | Ships with Windows 11 |

## Commands

```powershell
npm install                     # frontend deps
npm run tauri dev               # dev: Vite HMR + Rust
npm run tauri build             # release + NSIS installer
npm run tauri build -- --no-bundle   # release exe only
npm run check                   # svelte-check
npm run tauri icon <file>.png   # regenerate icons
```

First Rust build compiles librespot and Tauri from source — around 9 minutes.
Later builds touching only project code take about 5 minutes, dominated by LTO.

## Two build pipelines, joined at the end

```
Frontend                          Backend
────────                          ───────
index.html + src/**               src-tauri/src/**
  │ vite build                      │ cargo build --release
  ▼                                 │
dist/  ──────────────────────────► generate_context!() embeds dist/
                                    │
                                    ▼
                            spotify-rust.exe (13.13 MB)
```

`beforeBuildCommand` in [[tauri.conf.json]] runs `npm run build` first, so a
single `tauri build` does both.

> **Gotcha:** in a release build the frontend is embedded **at compile time**.
> Rebuilding `dist/` afterwards changes nothing until cargo recompiles the
> `spotify-rust` crate. In `dev` this does not apply — the webview loads from
> the Vite server, so HMR works normally.

## The `vergen` pin — do not remove

[[Cargo.lock]] pins **`vergen` 9.0.6**. Without it the project does not compile:

```
error[E0277]: the trait bound `vergen::feature::build::Build:
              vergen_lib::entries::Add` is not satisfied
error: could not compile `librespot-core` (build script)
```

`vergen` 9.1.0 (Jan 2026) ships `vergen-lib` 9.1.0, but `vergen-gitcl` — which
librespot-core 0.8.0 (Nov 2025) uses in its build script — expects the
`vergen-lib` 0.1.x line. Cargo resolves both and the build script gets the
wrong trait.

Restore with:

```powershell
cargo update -p vergen --precise 9.0.6
```

Re-check only once librespot has caught up.

## Release profile ([[Cargo.toml]])

```toml
opt-level = "s"      # optimise for size
lto = true           # cross-crate inlining, big win, slow build
codegen-units = 1    # better optimisation, no parallelism
strip = true         # drop symbols
```

Tuned for size and low runtime footprint over build speed — the target is
low-end hardware. `panic = "abort"` is deliberately **not** set: a panicking
background task (event pump, refresher) should not take down the app.

## Frontend build config ([[vite.config.ts]])

- `target: "chrome110"` — WebView2 is evergreen Chromium, so little
  transpilation is needed and the bundle stays small.
- `port: 1420, strictPort: true` — must match `devUrl` in [[tauri.conf.json]].
- `watch.ignored: ["**/src-tauri/**"]` — otherwise Rust rebuilds retrigger Vite.
- `sourcemap: false`, `minify: "esbuild"`.

Output: ~64 KB JS (23 KB gzipped), ~11 KB CSS.

## npm script approval

npm blocks install scripts by default here. `esbuild`'s postinstall must run or
Vite cannot build; the approval is recorded in [[package.json]] under
`allowScripts` and in [[package-lock.json]]. If `vite build` fails right after a
fresh clone:

```powershell
npm approve-scripts esbuild
npm rebuild esbuild
```

## Runtime configuration

There is **no config file**. Everything is compile-time constants or runtime
state:

| Setting | Where | Value |
| --- | --- | --- |
| OAuth client ID | [[auth.rs]] | librespot's default (Spotify desktop) |
| OAuth redirect | [[auth.rs]] | `http://127.0.0.1:8898/login` |
| Scopes | [[auth.rs]] | `SCOPES` constant |
| Connect device name | [[commands.rs]] | `<COMPUTERNAME> (spotify-rust)` |
| Audio cache cap | [[player.rs]] | 2 GB |
| Refresh margin | [[auth.rs]] | 5 min before expiry |
| Log level | [[lib.rs]] | `info,librespot=warn`; `RUST_LOG` overrides |

Runtime data lives in the Tauri app data dir: `tokens.json` (refresh token
only) and `cache/` (librespot credentials + audio).

## Security configuration

The CSP in [[tauri.conf.json]] restricts `img-src` to Spotify's CDNs
(`i.scdn.co`, `mosaic.scdn.co`, `image-cdn-*.spotifycdn.com`) and `connect-src`
to Tauri IPC only. The webview cannot reach the network directly — all HTTP
goes through Rust.

[[default.json]] grants the window a minimal permission set. Note that
capabilities gate the **frontend** IPC surface; Rust-side plugin use is
ungated, which is why media keys need no capability entry.

## Known build issues

| Symptom | Cause | Fix |
| --- | --- | --- |
| `vergen_lib::entries::Add` not satisfied | `vergen` unpinned | `cargo update -p vergen --precise 9.0.6` |
| `failed to bundle project: timeout: global` | NSIS download blocked | Retry on a good connection, or `--no-bundle` |
| `vite build` fails on esbuild | Install script not approved | `npm approve-scripts esbuild` |
| UI changes absent from release exe | `dist/` rebuilt after cargo | Re-run `tauri build` |

## See also

[[architecture]] · [[entry-points]] · [[external-dependencies]] ·
[[Cargo.toml]] · [[Cargo.lock]] · [[tauri.conf.json]] · [[vite.config.ts]] ·
[[package.json]] · [[MOC]]
