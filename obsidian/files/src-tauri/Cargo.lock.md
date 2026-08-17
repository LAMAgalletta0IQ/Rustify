---
tags: [file, config, build, generated]
---
# `src-tauri/Cargo.lock`

**Module:** [[tauri-config]] · **Language:** TOML (cargo-generated)

## Purpose

Records the exact resolved version of every crate in the dependency graph,
transitively. Machine-written — but **documented rather than excluded**, because
it carries one decision without which the project does not compile.

## The `vergen` pin — the reason this file matters

```toml
[[package]]
name = "vergen"
version = "9.0.6"
```

Without this pin, cargo resolves `vergen` 9.1.0 and the build fails:

```
error[E0277]: the trait bound `vergen::feature::build::Build:
              vergen_lib::entries::Add` is not satisfied
error: could not compile `librespot-core` (build script)
```

### Why

`vergen` 9.1.0 (published January 2026) ships alongside a `vergen-lib` 9.1.0.
But `vergen-gitcl` 1.0.8 — used by **librespot-core 0.8.0**'s build script
(published November 2025) — expects the `vergen-lib` **0.1.x** line. Cargo
happily resolves *both* `vergen-lib` versions into the graph, and the build
script ends up compiled against the wrong trait definition.

The failure is in a *dependency's build script*, so nothing in this project's
own source can work around it.

### Restoring the pin

```powershell
cargo update -p vergen --precise 9.0.6
```

Confirm afterwards that only `vergen-lib 0.1.6` remains:
```powershell
cargo tree -i vergen-lib@9.1.0    # should report no match
```

> **Do not run a blanket `cargo update`** without re-checking this. Revisit only
> once librespot releases a version built against the newer `vergen`.

## Inputs / outputs / side effects

Read and written by cargo. Committed deliberately: this is an application, not
a library, so reproducible builds are wanted.

## Dependencies

**Generated from:** [[Cargo.toml]]
**Consumed by:** cargo during every build

## Notable logic / gotchas

- **Do not hand-edit.** Use `cargo update -p <crate> --precise <version>`.
- The file is large (hundreds of packages) because librespot and Tauri each
  pull substantial trees — including `symphonia` for audio decoding and
  `windows-sys` for platform APIs.
- Deleting it and rebuilding **reintroduces the bug**, since resolution would
  pick the newest `vergen` again.

## See also

[[Cargo.toml]] · [[build-and-config]] · [[external-dependencies]] ·
[[excluded]] · [[tauri-config]] · [[MOC]]
