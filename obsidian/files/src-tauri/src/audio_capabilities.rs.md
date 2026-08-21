---
tags: [file, backend, audio, lossless, rust]
---
# `src-tauri/src/audio_capabilities.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **373 lines**

Modern Spotify audio-format and storage-resolution capability inspection —
deliberately separate from key acquisition. Metadata, format selection, v2
storage resolution and FLAC decoding are ordinary plumbing and are fully
implemented; Spotify-native FLAC **key derivation** currently depends on the
unavailable PlayPlay protected component and is never attempted here.

## Key items

### `struct AudioCapability`
Per-track file-format inventory (`AudioFormatCapability` list, preferred
format), `storage: StorageCapability`, and three honest booleans:
`losslessMetadataAvailable`, `losslessDecoderAvailable`,
`losslessPlaybackAvailable` (always `false`) plus `losslessBlocker`
(explains why, in user-facing language).

## Dependencies

**Imports:** `librespot::core::session::Session` — no crate-internal imports
beyond `error.rs`
**Imported by:** [[commands.rs]] (`get_audio_capability`)

## Notable logic / gotchas

> Never conflate "FLAC metadata/decoder present" with "Lossless plays". This
> file's whole design is keeping those three booleans genuinely independent
> so the UI can never accidentally claim more than what actually works.

## See also

[[commands.rs]] · [[Settings.svelte]] · [[NowPlaying.svelte]] ·
[[known-limitations]] · [[MOC]]
