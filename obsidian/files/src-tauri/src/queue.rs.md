---
tags: [file, backend, webapi, playback, rust]
---
# `src-tauri/src/queue.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **106 lines**

## Purpose

Reads and appends to the playback queue.

> **Why this module exists separately.** It is not one of the originally
> planned backend modules. Queue state has no clean home in [[player.rs]]
> because **librespot's `Spirc` exposes no queue *reader***, and it is pure Web
> API — so it lives on its own. The file's header comment says exactly this.

## Key items

### `struct QueueView`
`currently_playing: Option<TrackSummary>`, `queue: Vec<TrackSummary>`.

### Private wire types
`WireQueue`, `WireImage`, `WireNamed`, `WireAlbum`, `WireTrack`. `WireTrack`
marks `artists` and `duration_ms` `#[serde(default)]` because queue entries are
occasionally sparser than ordinary track objects.

### `async fn get_queue(api, token) -> QueueView`
`GET /me/player/queue`.

### `async fn add_to_queue(api, token, uri) -> ()`
`POST /me/player/queue?uri=<encoded>`. The URI goes in the **query string**,
not a JSON body — an API quirk.

### `fn urlencode(s: &str) -> String`
A small hand-rolled percent-encoder preserving the unreserved set
(`A-Z a-z 0-9 - _ . ~`). Written inline rather than pulling in the `urlencoding`
crate for a single call site.

### `impl From<WireTrack> for TrackSummary`
Note this coexists with an identically-named impl in [[library.rs]] — they
convert *different local* `WireTrack` types, so there is no conflict.

## Inputs / outputs / side effects

Network only. `add_to_queue` **modifies the user's active playback queue**.

## Dependencies

**Imports:** `serde`, `serde_json`, [[library.rs]] (`TrackSummary`),
[[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]]

## Notable logic / gotchas

- **The queue reflects the *active* device**, not necessarily this app. Correct
  while this app is active — the normal case — but it can show another device's
  queue after a transfer. Documented in the file header and
  [[known-limitations]].
- **No reorder or remove exists in the Web API.** Append is the only mutation
  possible. [[NowPlaying.svelte]] therefore implements "jump into the queue" by
  replaying it as an explicit track list via `load_tracks`.
- `POST /me/player/queue` answers `204`, handled by [[webapi.rs]].
- The hand-rolled encoder is correct for Spotify URIs (`spotify:track:...` →
  colons become `%3A`) but is not a general-purpose URL encoder.

## See also

[[playback-and-connect]] · [[NowPlaying.svelte]] · [[library.rs]] ·
[[webapi.rs]] · [[commands.rs]] · [[known-limitations]] · [[backend-rust]] ·
[[MOC]]
