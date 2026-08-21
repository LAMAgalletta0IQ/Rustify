---
tags: [file, backend, video, rust]
---
# `src-tauri/src/music_videos.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **345 lines**

Spotify-native music-video discovery and its protected-playback boundary.
Discovery is ordinary authenticated metadata: an audio track's
`VIDEO_ASSOCIATIONS` extension points to a video catalog track, whose
`original_video.gid` is the v9 manifest id. Actual playback is
PlayReady-protected and is **intentionally not attempted** — this file
stops at capability detection.

## Key items

### `struct MusicVideoCapability { available, video_uri, images, playback_supported, playback_blocker }`
`playback_supported` is always `false`; `playback_blocker` explains why in
user-facing language, surfaced in Now Playing rather than silently omitted.

## Dependencies

**Imports:** `librespot::core::session::Session` — no crate-internal imports
beyond `error.rs`
**Imported by:** [[commands.rs]] (`get_music_video_capability`)

## See also

[[commands.rs]] · [[NowPlaying.svelte]] · [[known-limitations]] · [[MOC]]
