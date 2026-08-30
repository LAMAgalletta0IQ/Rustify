---
tags: [file, backend, rust, limitations]
---
# `src-tauri/src/lastfm.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~290 lines**

Optional Last.fm enrichment for [[dna.rs]]. The only place in Rustify that
talks to Last.fm at all.

## Why it exists

Spotify's `genres` array on `/me/top/artists` is the weakest input to Listening
DNA: it is **empty** for a large share of artists, reliably so for smaller
ones. A listener with niche taste therefore gets a flatter shape than a
mainstream one for reasons that have nothing to do with their listening.
`artist.getTopTags` fills that gap with denser, more descriptive community tags.

## Why it is optional

It needs an API key the user registers themselves, so it can never be a
requirement. With no key configured this module is **not called at all** and
DNA behaves exactly as it did before the option existed.
`ListeningDna::tag_source` reports which happened — and reports what was
*used*, not what was configured, so a key that returned nothing still reads
`"spotify"` rather than advertising an enrichment that didn't occur.

## Scope of what is sent

- **Unauthenticated reads only.** `artist.getTopTags` takes the API key alone.
  The shared secret and the auth handshake are not involved, so nothing here
  can scrobble or act on the user's account.
- **No username is sent** — only artist names the user already has on screen.
- **No tempo or mood.** Last.fm does not publish those. This does not
  substitute for the withdrawn Spotify audio-features endpoint (see
  [[known-limitations]]); it only improves tag coverage.

## Key items

### `async fn enrich_tags(http, cache, api_key, artists) -> HashMap<..>`
Tags for up to `MAX_ARTISTS` (20) names, four requests in flight, under a
`TOTAL_TIMEOUT` (6 s) budget for the whole pass. **Never returns an error.**
Every failure mode — no network, a rejected key, a slow response past the
budget — yields fewer entries, and the caller treats a missing entry as "use
Spotify's tags for this artist".

### `const NON_GENRE_TAGS`
Last.fm's vocabulary is user-generated and full of terms describing the
*listener's relationship* to the artist rather than the music. `seen live`,
`female vocalists` and `00s` are among the most-applied tags on the whole site;
left in, they would outrank every real genre.

Matched against the **whole normalised tag, never as substrings** — a substring
rule on `uk` or `love` would silently remove `uk garage` and `lovers rock`, and
one on `rock` or `pop` would be catastrophic. A unit test pins this.

### `struct LastfmCache`
Per-artist, keyed by lowercased name (there is no Spotify id on the other
side), 24 h TTL, held on [[state.rs]] so it outlives one Profile visit.
Cleared when the key changes or is removed, so a rejected key's empty answers
are not remembered as real ones.

## Notes

- `MIN_TAG_COUNT` (20) drops long-tail noise; `MAX_TAGS_PER_ARTIST` caps each
  artist's contribution so a heavily-tagged famous artist cannot outweigh an
  obscure one — that would be a popularity effect masquerading as taste.
- The key is stored in `settings.json` next to the Spotify Client ID, in
  plaintext. There is no environment override, unlike `RUSTIFY_CLIENT_ID`:
  this is a user preference, not a packaging concern.

## Dependencies

**Imports:** `reqwest` · `futures_util` · `serde` · `tokio::sync::RwLock`
**Imported by:** [[dna.rs]] (`enrich_tags`) · [[state.rs]] (`LastfmCache`) ·
[[commands.rs]] (`get_lastfm_config`, `set_lastfm_api_key`,
`clear_lastfm_api_key`)

## See also

[[dna.rs]] · [[Settings.svelte]] · [[Profile.svelte]] · [[known-limitations]] ·
[[2026-08-fixes-pass]] · [[MOC]]
