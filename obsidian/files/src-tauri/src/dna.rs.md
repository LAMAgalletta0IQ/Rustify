---
tags: [file, backend, library, rust, limitations]
---
# `src-tauri/src/dna.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~330 lines**

A taste profile ("Listening DNA") derived from the user's top artists and
tracks, rendered as a radar chart in [[Profile.svelte]].

## Why this is not Spotify's DNA

Spotify's own feature is built on `GET /v1/audio-features`, which is **closed
to apps in Development Mode** — the state every self-registered Client ID is in
until it passes an extension review. Rustify's whole auth design is that each
user registers their own app (see [[auth-and-tokens]]), so those numbers are
unreachable, and reading them out of the web player instead is both against the
Developer Terms and a reliable way to get IP-banned. See [[known-limitations]].

What is still available on the ordinary Web API:

- `/me/top/artists` → `genres` (free-text tags) and `popularity` (0-100)
- `/me/top/tracks` → the album's `release_date`

Every axis is computed from those alone, and each carries a `basis` string
naming what was measured. The axis names deliberately describe the measurement
(`Mainstream` = mean artist popularity) rather than borrowing Spotify's
psychoacoustic vocabulary.

## Key items

### `async fn listening_dna(api, token, current_year) -> ListeningDna`
Two 50-item requests (the endpoints' maximum), then six axes:

| Axis | Measured as |
| --- | --- |
| Mainstream | mean `popularity` of the top artists |
| Variety | distinct genre tags against `VARIETY_CEILING` (30) |
| Freshness | share of top tracks released within `FRESH_YEARS` (2) |
| Intensity | share of genre tags matching `INTENSE_TAGS` |
| Mellow | share of genre tags matching `MELLOW_TAGS` |
| Loyalty | share of top tracks belonging to the five most-frequent artists |

An empty response degrades the affected axis to 0 rather than failing — a new
account genuinely has no top tracks, and that is not an error. `sparse` is set
below five artists or tracks so the UI can decline to draw a confident-looking
shape from almost nothing.

### `fn tag_share(tags, needles)`
Share of **all** tags matching any needle, not share of matching tags. A
listener whose tags are 90% "ambient" must not score the same on Intensity as
one whose tags are 90% "metal" merely because both have one matching tag; the
unit test pins this.

### `fn release_year(date)`
Leading four digits of a Spotify `release_date`, which is `YYYY`, `YYYY-MM` or
`YYYY-MM-DD` depending on `release_date_precision`.

## Notes

- `INTENSE_TAGS`/`MELLOW_TAGS` match as **substrings**, case-insensitively:
  Spotify's genre tags are open compounds ("melodic death metal", "uk drill"),
  not a closed vocabulary.
- `current_year` comes from the webview so the freshness axis follows the
  user's calendar rather than UTC's.
- Two extra Web API requests per Profile open. Not a recurring timer, so it is
  outside the concern in [[rate-limiting]], but it is why the frontend loads
  this separately from the rest of the profile rather than blocking on it.

## Dependencies

**Imports:** `serde` · `error.rs` · [[webapi.rs]]
**Imported by:** [[commands.rs]] (`get_listening_dna`)

## See also

[[Profile.svelte]] · [[known-limitations]] · [[2026-08-fixes-pass]] ·
[[library.rs]] · [[MOC]]
