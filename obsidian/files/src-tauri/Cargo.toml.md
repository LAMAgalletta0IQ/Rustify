---
tags: [file, config, build, backend]
---
# `src-tauri/Cargo.toml`

**Module:** [[tauri-config]] · **Language:** TOML

## Purpose

The Rust crate manifest: package identity, both build targets, every
dependency, and the size-tuned release profile.

## Key items

### `[package]`
`name = "rustify"`, `version = "0.1.0"`, `edition = "2021"`,
`rust-version = "1.82"` (verified building on 1.97.1).

### `[lib]`
```toml
name = "rustify_lib"
crate-type = ["staticlib", "cdylib", "rlib"]
```
The `_lib` suffix keeps the library target from colliding with the binary — this
is the name [[main.rs]] calls into. The three crate types are the Tauri 2 template
default, supporting desktop and (hypothetically) mobile linking.

### `[build-dependencies]`
`tauri-build` — drives [[build.rs]].

### `[dependencies]`
See [[external-dependencies]] for the full table and how each is used. The
selection worth noting here:

```toml
librespot = { version = "0.8.0", default-features = false,
              features = ["native-tls", "rodio-backend"] }
librespot-oauth = { version = "0.8.0", default-features = false,
                    features = ["native-tls"] }
```

> **`default-features = false` is deliberate and commented in the file.** The
> dropped default is **`with-libmdns`** — zeroconf discovery, needed only for
> the "log in from another device" flow. This app authenticates by OAuth and
> registers as a Connect device through Spotify's dealer/websocket, so mDNS is
> a dependency tree that never executes.

`reqwest` also uses `default-features = false` with `native-tls`, matching
librespot so only one TLS stack is compiled in.

`dotenvy` loads `.env` at startup ([[lib.rs]]). Tiny, and the only way the app
takes configuration from outside the binary — see [[build-and-config]].

### `[profile.release]`
```toml
opt-level = "s"      # size over speed
lto = true           # cross-crate inlining
codegen-units = 1    # better optimisation, no parallelism
strip = true         # drop symbols
```
Produces a 13.13 MB binary. `lto` plus `codegen-units = 1` are why release
builds take ~5–9 minutes.

## Inputs / outputs / side effects

Read by cargo. Determines what is compiled and linked.

## Dependencies

**Depends on:** crates.io; [[Cargo.lock]] pins the resolved versions
**Depended on by:** the whole Rust build; [[build.rs]] runs as its build script

## Notable logic / gotchas

- **`panic = "abort"` is deliberately absent.** It would shrink the binary
  further, but a panicking background task — the event pump in [[player.rs]] or
  the refresher in [[auth.rs]] — should not take down the whole app.
- **Changing the `[lib] name` breaks [[main.rs]]**, which calls
  `rustify_lib::run()` by that exact name.
- **`futures-util` is declared but barely used**; harmless, and a candidate for
  removal.
- Version specs are loose (`"2"`, `"1"`); reproducibility comes from
  [[Cargo.lock]], which carries the load-bearing `vergen` pin described in
  [[build-and-config]].

## See also

[[Cargo.lock]] · [[build.rs]] · [[external-dependencies]] ·
[[build-and-config]] · [[main.rs]] · [[tauri-config]] · [[MOC]]
