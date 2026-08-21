---
tags: [file, backend, jam, rust]
---
# `src-tauri/src/jams/config.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **142 lines**

Runtime configuration for the Jams module. The service *hosts* are stable
and hardcoded; the SpClient *endpoint paths* ship with defaults
(`JamConfig::default`) that can be overridden per-key from `jams.toml` in
the app data dir, because they're internal paths that can change without
notice. Pathfinder persisted-query hashes have **no** defaults — a wrong
hash is rejected outright by Spotify — so metadata enrichment (concert/
artist enrichment on top of a jam) stays off until one is captured and
added to `jams.toml`.

## Key items

### `fn default_spclient_endpoints() -> HashMap<...>`
The five stable `social-connect/v2/sessions/...` paths (create, current,
join, leave, add track) — see `README_jams.md` for the exact table.

### `struct JamConfig`
Loaded via `JamConfig::from_file(app_data_dir.join("jams.toml"))`, falling
back to `default()` if absent. A `[spclient_endpoints]` table containing
only one key overrides just that key, not the whole set.

## Dependencies

**Imports:** `serde`, `toml` — no crate-internal imports
**Imported by:** [[mod.rs|jams/mod.rs]], [[jams_bridge.rs]]

## See also

`README_jams.md` (the endpoint table and why they're overridable) ·
[[jams_bridge.rs]] · [[mod.rs|jams/mod.rs]] · [[MOC]]
