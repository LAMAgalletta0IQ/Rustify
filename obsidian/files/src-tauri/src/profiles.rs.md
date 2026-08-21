---
tags: [file, backend, spclient, profile, rust]
---
# `src-tauri/src/profiles.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **428 lines**

Rich Spotify user profiles through the authenticated SpClient session. The
public Web API only exposes the current account and has no user-search
surface; Spotify's own clients use `user-profile-view/v3` for both the
current user and profiles reached from playlists/friend activity.

## Key items

### `fn normalize_username(value) -> AppResult<String>`
Accepts a bare username, a `spotify:user:` URI, or an
`https://open.spotify.com/user/...` URL. Real URL parsing (`reqwest::Url`),
not string splitting — rejects host lookalikes (`open.spotify.com.evil`),
scheme/userinfo/port abuse, and bounds length/control characters.

### `fn safe_image_url(value) -> Option<String>`
`https://` only, valid host required — applied to every profile/relation
image URL.

### `async fn fetch(session, username_or_uri) -> AppResult<UserProfile>`
Fetches the profile plus followers/following concurrently; each relation
list's own failure degrades to an empty list with `followers_available`/
`following_available: false` rather than failing the whole profile — the
two are distinguished so the UI can show "private/unavailable" instead of
silently rendering an empty list as if there were no followers.

## Dependencies

**Imports:** `http`, `librespot::core::{session::Session, SpotifyUri}`
— no crate-internal imports beyond `error.rs`
**Imported by:** [[commands.rs]]

## Notable logic / gotchas

> ### `normalize_username` had a lifetime bug at merge time
> An in-progress rewrite (URL parsing added, replacing plain string
> splitting) left `segments[1]` — a `&str` borrowed from a local `url` —
> outlasting `url`'s own scope, a straightforward "borrowed value does not
> live long enough" compile error caught immediately by `cargo check`, not a
> live-behavior bug. Fixed by making the branch build an owned `String`
> before `url` drops.

## See also

[[commands.rs]] · [[Profile.svelte]] · [[MOC]]
