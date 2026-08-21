---
tags: [file, backend, lyrics, integration]
---
# `src-tauri/src/lyrics.rs`

Read-only documented LRCLIB client. `fetch` calls `https://lrclib.net/api/get`
with exact track, first artist, album, and rounded duration parameters, a
12-second timeout, and an identifying Rustify repository User-Agent. It never
scrapes Spotify, stores credentials, or calls a private endpoint.

Result status is `available`, `instrumental`, or `unavailable`. A 404 is a
normal unavailable result, a 429 becomes typed `RateLimited` with
`Retry-After`, and provider 5xx becomes `ServiceUnavailable`.

`parse_lrc` supports multiple timestamps on one line, ignores metadata tags,
converts timestamps to milliseconds, preserves blank timing rows, and sorts.
Its unit test covers multiple timestamps and out-of-order input.

## See also

[[NowPlaying.svelte]] · [[external-dependencies]] · [[2026-08-capability-audit]] · [[MOC]]
