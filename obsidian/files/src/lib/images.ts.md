---
tags: [file, frontend, typescript, library]
---
# `src/lib/images.ts`

**Module:** [[frontend-svelte]] · **Language:** TypeScript · **~85 lines**

Cover-art encoding for playlist uploads. One export.

## Why it exists

`PUT /playlists/{id}/images` accepts **base64 JPEG only**, capped at 256 KB of
*base64* (~192 KB of image). A file the user picks is typically an order of
magnitude past that and often a PNG besides, so it has to be re-encoded before
it can be sent.

Doing that in the webview rather than in Rust means the backend needs no image
codec at all — [[library.rs]]'s `update_playlist_image` just forwards bytes,
and no new crate enters the dependency tree for one endpoint.

## Key items

### `async function encodeCoverJpeg(file): Promise<string>`
Centre-crops to a square (Spotify renders every cover as one, so cropping here
shows the user the framing the app will use), downscales to at most 640px, then
walks `QUALITY_STEPS` (0.9 → 0.4) until the encoding fits `MAX_COVER_BASE64`.

The ladder is not a single fixed quality: JPEG size at a given quality varies
enormously with image content, so one guess either wastes headroom or
overshoots. Throwing only happens when even 0.4 is too large, which in practice
means an image that is noise at full frame.

Returns bare base64 with no `data:` prefix. The Rust side accepts either form.

### `const MAX_COVER_BASE64`
Mirrors `library::MAX_PLAYLIST_IMAGE_BASE64`. Both sides check, so an oversized
payload fails locally with a useful message rather than as an opaque 413.

## Dependencies

**Imports:** none (DOM `Image`/`canvas` only)
**Imported by:** [[PlaylistView.svelte]]

## See also

[[PlaylistView.svelte]] · [[library.rs]] · [[api.ts]] · [[2026-08-fixes-pass]] ·
[[MOC]]
