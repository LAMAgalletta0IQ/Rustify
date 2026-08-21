---
tags: [file, config, build, backend, rust]
---
# `src-tauri/build.rs`

**Module:** [[tauri-config]] · **Language:** Rust · **3 lines**

## Purpose

The cargo build script. Runs before the crate compiles and lets `tauri-build`
do its code generation and resource embedding.

## Contents

```rust
fn main() {
    tauri_build::build()
}
```

## What `tauri_build::build()` actually does

Despite the size, this call is responsible for a lot:

1. **Reads [[tauri.conf.json]]** and validates it against the Tauri schema.
2. **Processes [[default.json]]** and writes the generated ACL and permission
   schemas into `src-tauri/gen/schemas/` (excluded from this vault — see
   [[excluded]]).
3. **Embeds the Windows resource**: `icons/icon.ico` from [[icons]], plus
   version metadata, so the built `.exe` carries a proper icon and properties.
4. **Emits `cargo:rerun-if-changed` directives**, which is why editing
   [[tauri.conf.json]] or the capability file triggers a Rust rebuild.

## Inputs / outputs / side effects

**Inputs:** [[tauri.conf.json]], `capabilities/*.json`, `icons/icon.ico`
**Outputs:** `src-tauri/gen/schemas/*.json`, a Windows resource object,
cargo directives
**Side effects:** writes into `gen/` and `target/` at build time

## Dependencies

**Imports:** `tauri_build` (from `[build-dependencies]` in [[Cargo.toml]])
**Runs for:** the `rustify` crate, before [[lib.rs]] compiles

## Notable logic / gotchas

- **A missing `icons/icon.ico` fails the build here**, not at link time. This is
  why `npm run tauri icon` must run before the first build on a fresh clone —
  the error surfaces from this script.
- **`generate_context!()` in [[lib.rs]] depends on this having run**; the macro
  consumes what the build script prepared.
- The generated `gen/schemas/` output is derivative and safe to delete — it is
  rewritten on the next build.
- Editing [[tauri.conf.json]] invalidates the crate, so a "frontend-only"
  config change still costs a Rust recompile.

## See also

[[tauri.conf.json]] · [[default.json]] · [[Cargo.toml]] · [[lib.rs]] ·
[[icons]] · [[build-and-config]] · [[excluded]] · [[tauri-config]] · [[MOC]]
