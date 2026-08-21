---
tags: [file, backend, podcast, rust]
---
# `src-tauri/src/podcasts.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **417 lines**

Podcast episode resume state through Spotify's Herodotus resumption
platform. Bodies are ordinary protobuf (not gRPC-framed); this module keeps
a deliberately small hand-written wire implementation for the nine fields
it actually consumes rather than pulling in a second generated protocol
tree, skipping unknown fields safely for forward compatibility.

## Key items

### `struct EpisodeResume { position_ms, completed, has_state, ... }`
The resume state surfaced to the UI (Now Playing's episode badge).

### Automatic checkpointing
Wired into playback (periodic + pause/seek/stop/logout checkpoints, per
[[player.rs]]/[[commands.rs]]) so resume position tracks real listening,
not just explicit user action.

## Dependencies

**Imports:** hand-rolled protobuf varint/field decoding, `librespot::core::session::Session`
**Imported by:** [[commands.rs]] (`get_episode_resume`, `set_episode_completed`)

## See also

[[commands.rs]] · [[NowPlaying.svelte]] · [[MOC]]
