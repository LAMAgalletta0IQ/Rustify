---
tags: [file, backend, entrypoint, rust]
---
# `src-tauri/src/main.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **6 lines**

## Purpose

The OS process entry point. Deliberately almost empty — all logic lives in
[[lib.rs]] so the same startup path can be reused from a library context (and,
in principle, a mobile entry point).

## Contents

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    spotify_rust_lib::run()
}
```

### The `windows_subsystem` attribute

On **release** builds this marks the binary as a GUI application, so Windows
does not allocate a console window. On debug builds the attribute is omitted
(`not(debug_assertions)`), keeping the console so `env_logger` output is
visible during development.

This conditional is why `cargo run` shows logs but a double-clicked release exe
does not. To see logs from a release build, run it from a terminal that
captures stdout, or redirect to a file.

### `spotify_rust_lib`

The crate name comes from `[lib] name = "spotify_rust_lib"` in [[Cargo.toml]].
The package is `spotify-rust`, but Rust library names cannot contain hyphens,
hence the underscored alias.

## Inputs / outputs / side effects

None of its own. Delegates immediately; `run()` blocks until the window closes.

## Dependencies

**Imports:** the crate's own library target ([[lib.rs]])
**Imported by:** nothing — this is the root of the binary target

## Notable logic / gotchas

- Splitting `main.rs` from `lib.rs` is the standard Tauri 2 template layout,
  not incidental. `[lib] crate-type = ["staticlib", "cdylib", "rlib"]` in
  [[Cargo.toml]] exists to support it.
- There is no error handling here: `run()` itself `expect()`s, so a failure to
  start the Tauri runtime panics with a message rather than returning a code.

## See also

[[lib.rs]] · [[entry-points]] · [[Cargo.toml]] · [[backend-rust]] · [[MOC]]
