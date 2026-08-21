---
tags: [file, backend, library, rust]
---
# `src-tauri/src/relevance.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **173 lines**

Local, per-account "quick access" ranking. Not a Spotify API — a small
JSON file (`relevance-<sanitized-account-id>.json`) in the app data dir
recording opens/plays/last-action-time per item, used to rank Home's
Quick Access row.

## Key items

### `fn record(data_dir, account, item, is_play)`
Increments `opens` or `plays` and updates `last_action_ms` for one item,
read-modify-write against the account's file.

### `fn rank(...)`
Repeated recent contexts rank above single older items; the exact scoring
combines recency and frequency (see the function itself and its tests).

## Dependencies

**Imports:** `serde_json`, `std::fs` — no crate-internal imports beyond
`error.rs`, [[library.rs]] (`RecentActivityItem`)
**Imported by:** [[commands.rs]] (`record_relevance`, `get_quick_access`)

## See also

[[commands.rs]] · [[Home.svelte]] · [[MOC]]
