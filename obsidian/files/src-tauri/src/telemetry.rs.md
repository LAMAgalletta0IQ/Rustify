---
tags: [file, backend, telemetry, rust]
---
# `src-tauri/src/telemetry.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **345 lines**

Privacy-preserving playback-telemetry boundary. Spotify's current
account-history transport is the private Gabo receiver; the only way to get
third-party traffic accepted there is impersonating desktop-client context
and anti-fraud fields. **Rustify deliberately does not do that.** This
module still records the exact, genuine local playback facts (bounded
ledger — actual elapsed listening time, never seek distance, never
pause time) so the plumbing is ready if a legitimate delivery path is ever
available, without inventing or exaggerating anything.

## Key items

### `struct TelemetryTracker`
Bounded in-memory ledger of real local playback, owned by [[state.rs]]'s
`AppState`.

### `struct TelemetryStatus { delivery_available: false, delivery_blocker, active_playbacks, locally_recorded }`
Surfaced in Settings, worded honestly (`delivery_available` is always
`false`, with the reason in `delivery_blocker`) rather than hidden.

## Dependencies

**Imports:** `std::time` — no crate-internal imports beyond `error.rs`
**Imported by:** [[commands.rs]] (`get_telemetry_status`), [[player.rs]] (records real playback)

## See also

[[commands.rs]] · [[Settings.svelte]] · [[known-limitations]] · [[MOC]]
