# Task completion
From repository root:
1. `npm run check`
2. `npm run build`
3. `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
4. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
5. `cargo test --manifest-path src-tauri/Cargo.toml`
6. `cargo check --manifest-path src-tauri/Cargo.toml`
7. `npm run tauri build` when packaging is in scope (full release + NSIS; LTO can take 6+ minutes).
For UI/runtime changes: launch `npm run tauri dev`, connect Tauri MCP to 127.0.0.1:9223, inspect accessibility DOM/computed geometry/screenshots/console + Rust logs, and exercise success/partial/error/responsive/focus states. Preserve remote playback unless transfer is explicitly necessary. Reversible library tests must restore original server state. Update Obsidian and Serena memory before handoff.