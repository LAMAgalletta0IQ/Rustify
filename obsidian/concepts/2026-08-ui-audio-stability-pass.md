---
tags: [concept, verification, ui, audio, lyrics, architecture]
---
# 2026-08 UI and audio stability pass

This pass repaired Settings audio behavior, menu consistency, player
navigation, fullscreen lyrics, and feature ownership without moving or
reformatting the protected experimental Jam implementation.

## Root causes

- Equalizer controls edited a frontend draft but the native sink changed only
  when the whole Settings form was saved.
- A saved output device was rejected if it was temporarily disconnected, so
  the sink's existing fallback/retry logic could never be used.
- Settings and the player did not share native-output state or presentation.
- Native selects and component-local absolute popovers caused inconsistent
  styling, clipping, and focus behavior.
- Artwork and the full Now Playing surface shared an action; fullscreen lyrics
  had no explicit entry point.
- The page duplicated transport/timeline controls and used
  `scrollIntoView`, which could scroll ancestors and overlap smooth-scroll
  operations.
- Queue loading reacted to every replacement playback snapshot even when the
  track URI was unchanged, unnecessarily consuming Web API quota.

## Resulting boundaries

- `features/audio/` owns shared CPAL enumeration and output selection.
- `features/player/` owns the persistent player and Spotify Connect menu.
- `features/lyrics/` owns Now Playing/fullscreen and lyric interaction.
- `features/settings/` owns the settings page.
- `ui/SelectMenu.svelte` is the shared accessible listbox/menu surface.
- `src-tauri/src/audio/mod.rs` owns output fallback and the DSP pipeline.
- `src-tauri/src/lyrics/mod.rs` owns LRCLIB lookup/parsing.

The Spotify Connect menu is intentionally separate from the CPAL output menu:
the former transfers the Spotify playback session; the latter swaps the sink
under the same local session.

## Audio contract

`configure_audio` validates and applies an output/EQ snapshot without disk
I/O. `update_audio_settings` persists the same snapshot. Slider changes use
the first immediately and debounce the second. The six peaking filters,
35 ms coefficient/wet smoothing, clamp, preamp, and automatic headroom remain
inside the real librespot `Sink` wrapper.

Disconnected saved outputs remain selected but unavailable in the UI. The sink
uses the system default and retries the requested device every five seconds.
The former Settings "default volume" is now private
`last_volume_percent` session state with a serde alias for migration.

## Verification

- Svelte check and production Vite build: clean.
- Rust tests: 14 passed, including spectral 1 kHz gain, exact settled bypass,
  rapid curve stability, and old-volume-key migration.
- Live Tauri: preset persistence, custom preset create/delete, disconnected
  output representation, real headset↔Realtek↔default switching, album
  navigation, lyric seek, manual-follow suspension/resume, repeated
  fullscreen entry/exit, 820×620 and 1920×1080 layouts, and one timeline.
- Screenshots stayed under ignored `/screenshots/`.

Strict `cargo fmt --check` and `cargo clippy -D warnings` still report only
the pre-existing protected Jam formatting/lints; ordinary Clippy succeeds.

## See also

[[architecture]] · [[playback-and-connect]] · [[state-and-events]] ·
[[frontend-components]] · [[backend-rust]] · [[MOC]]
