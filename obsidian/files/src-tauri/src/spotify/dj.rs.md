---
tags: [file, backend, dj, rust]
---
# `src-tauri/src/spotify/dj.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **504 lines**

Spotify DJ X: resolves the dynamic `your_dj` Lexicon context into a real
track list plus narration metadata, and (partially) prepares narration
audio. Music playback works end-to-end; narration audio does not — see the
gotcha below, this is a genuine gap, not a documentation lag.

## Key items

### `pub(crate) async fn resolve(...) -> AppResult<DjSession>`
Resolves the DJ context via Lexicon (`hm://` context resolution), reason
`"interactive"` for a fresh start or `"state_restore"` for session restore.

### `fn parse_track(value) -> Option<DjTrack>`
Normalizes `spotify:media:<id>` to `spotify:track:<id>`, prefers
`canonical_track_uri` from metadata when present, rejects anything that
isn't a `spotify:track:`/`spotify:episode:` URI or the `spotify:delimiter`
sentinel. This is why DJ music playback actually works: by the time
[[commands.rs]]'s `start_dj` calls `Spirc.load()`, every URI is a normal,
resolvable Spotify URI — no different from loading any other track list.

### `struct DjSession { narration_resolved, narration_playback_supported, ... }`
Two separate booleans on purpose: `narration_resolved` means the TTS
fulfillment endpoint accepted the request (a capability probe);
`narration_playback_supported` means audio actually plays. As of 2026-08
the second is always `false` — see below.

### `async fn prepare_first_narration(...) -> AppResult<bool>`
Posts to `client-tts/v1/fulfill`, gets back a real, valid, short-lived
**signed HTTPS audio URL** in the response's `Location` header — and
discards it. Returns only whether one was obtained.

### `pub(crate) fn refill_uris(previous, refreshed) -> Vec<String>`
Bounded low-queue replenishment: excludes tracks already queued or just
played, used by [[../commands.rs|commands.rs]]'s DJ queue-refill path.

## Dependencies

**Imports:** `reqwest`, `serde_json` — no crate-internal imports beyond `error.rs`
**Imported by:** [[mod.rs|spotify/mod.rs]] (`InternalSpotify::resolve_dj`,
`prepare_dj_narration`, `dj_refill_uris`)

## Notable logic / gotchas

> ### Narration audio: resolved, never played
> `prepare_first_narration`'s doc comment says outright: "Audio injection
> consumes this same seam in the hardening pass" — i.e. this was always
> planned as follow-up work, not a finished feature with a bug. Playing an
> arbitrary signed HTTPS URL needs a **second** audio pipeline running
> alongside librespot's own `Sink` ([[../audio/mod.rs|audio/mod.rs]]'s
> `ProcessingSink` is a librespot `Sink` implementation specifically, not a
> general player) with real output-device coordination — attempting that
> without live audio hardware to verify against would risk shipping broken
> audio, so it's deliberately left as the honest, surfaced gap it is (see
> [[Home.svelte]]'s DJ card, which says so in plain language) rather than
> guessed at.

## See also

[[../commands.rs|commands.rs]] (`start_dj`) · [[Home.svelte]] ·
[[known-limitations]] · [[MOC]]
