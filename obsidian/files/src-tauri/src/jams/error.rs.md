---
tags: [file, backend, jam, rust]
---
# `src-tauri/src/jams/error.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **71 lines**

Error type for the Jams module. Everything public in `jams/` returns
`Result<_, JamError>`. Variants map onto the distinct failure modes of the
three internal services (SpClient, Pathfinder, Dealer) so callers react to
each without matching on error strings — mirrored onto the app's own
`AppError` by a `From<JamError>` impl in [[error.rs|src-tauri/src/error.rs]]
(`JamNotFound` → `Unavailable`, `PermissionDenied` → `Forbidden`,
`ClientTokenExpired` → `Auth`, `Config` → `BadRequest`).

## Key items

`enum JamError` — see the type itself for the full variant list; the ones
that matter for UI branching are `JamNotFound` (normal "not in a jam"
answer, e.g. a 404 on the current-session check) and `PermissionDenied`
(a real RBAC refusal, not a missing capability toggle).

## Dependencies

**Imports:** `thiserror` — no crate-internal imports
**Imported by:** every other file in `jams/`; [[error.rs|src-tauri/src/error.rs]] for the `AppError` mapping

## See also

[[mod.rs|jams/mod.rs]] · [[error.rs|src-tauri/src/error.rs]] · [[MOC]]
