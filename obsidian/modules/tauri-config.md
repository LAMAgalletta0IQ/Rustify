---
tags: [module, config]
---
# Module — Tauri configuration

**Path:** `src-tauri/` (excluding `src/`)

Everything that tells Tauri and Cargo how to assemble the desktop app.

## Files

| File | Role |
| --- | --- |
| [[tauri.conf.json]] | Window, CSP, dev/build commands, bundle targets |
| [[Cargo.toml]] | Crate manifest, dependencies, release profile |
| [[Cargo.lock]] | Exact versions — **pins `vergen` 9.0.6**, load-bearing |
| [[build.rs]] | Cargo build script; runs `tauri_build::build()` |
| [[default.json]] | Capability grant for the main window |
| [[icons]] | Generated icon set |

## How they interact

```
cargo build
  │
  ├─► build.rs → tauri_build::build()
  │       reads tauri.conf.json + capabilities/default.json
  │       writes src-tauri/gen/schemas/   (generated, see [[excluded]])
  │       embeds icons/icon.ico as a Windows resource
  │
  └─► compiles src/**  with generate_context!()
          embeds ../dist  (release only)
```

`build.rs` is why editing [[tauri.conf.json]] triggers a Rust rebuild.

## Security model

Two independent layers:

**CSP** ([[tauri.conf.json]]) — restricts what the webview may load.
`img-src` allows only Spotify's CDNs; `connect-src` allows only Tauri IPC. The
webview cannot make network requests, so all HTTP is forced through Rust.

**Capabilities** ([[default.json]]) — grants the window a minimal permission
set (`core:default`, `core:event:default`, window dragging, `opener:default`).

> Capabilities gate the **frontend** IPC surface only. Rust-side plugin usage
> is ungated — which is why [[media_keys.rs]] needs no capability entry despite
> using the global-shortcut plugin.

## Platform targeting

Windows only. `bundle.targets` is `["nsis"]`; the toolchain is
`x86_64-pc-windows-msvc`. The iOS and Android icons under [[icons]] are an
incidental by-product of `tauri icon`, not a sign of mobile support.

## See also

[[build-and-config]] · [[backend-rust]] · [[external-dependencies]] ·
[[entry-points]] · [[excluded]] · [[MOC]]
