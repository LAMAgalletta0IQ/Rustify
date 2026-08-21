---
tags: [file, backend, jam, rust]
---
# `src-tauri/src/jams/session.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **657 lines**

The typed jam model (`JamSession`, `JamMember`, `JamTrack`) and
`JamManager`, the module's public façade — create/join/leave/kick/end/
queue-control, all funnelled through `session_from_value`, which maps a
v2/v3 social-connect response onto the stable model while retaining the
untouched payload for newly-introduced fields Spotify hasn't documented.

## Key items

### `fn session_from_value(raw) -> Result<JamSession, JamError>`
Tolerant parser: tries both snake_case and camelCase key spellings (the v2
and v3 endpoints don't agree), requires only `session_id`/`sessionId`/`id`,
defaults everything else.

### `fn resolve_join_token(input) -> Result<String, JamError>`
Reduces whatever the user pasted to a join token, following a
`spotify.link/<id>` shortlink first if that's what they pasted — the
shortlink's last path segment is an **11-character shortlink id**, unrelated
to the session; the real join token only exists in the URL the shortlink
redirects to.

### `join_uri` / `join_url` construction
Prefers a `join_session_uri`/`join_session_url` field directly from
Spotify's response; falls back to constructing
`spotify:socialsession:<token>` / `https://open.spotify.com/socialsession/<token>`
from `join_session_token` if those fields aren't present. The constructed
form is not a guess — confirmed correct against both this module's own
PyPI-package capture and an independent OSS implementation
(`t1mlange/spotify-group-session`) doing the identical construction.

## Dependencies

**Imports:** `serde_json`, `tokio::sync::mpsc` — no crate-internal imports
**Imported by:** [[mod.rs|jams/mod.rs]], [[jams_bridge.rs]], [[spclient.rs]]

## Notable logic / gotchas

> ### The "weird invite link" investigation (2026-08)
> A user reported Rustify's own hosted-jam invite link looked different from
> ones shared by the official app. Two independent sources (this module's
> PyPI capture, and `t1mlange/spotify-group-session`) confirm the
> `open.spotify.com/socialsession/<token>` construction is genuinely
> correct — not the bug. More likely explanation, unconfirmed: the official
> app runs its link through a `spotify.link` URL shortener before showing
> it to the user, and Rustify shows the raw underlying link instead, which
> still works (a friend successfully joined via it) but looks unfamiliar.
> Widened the field-candidate list and added a debug log of the response's
> top-level keys on fallback, so the next live jam creation can confirm
> whether Spotify's own response ever carries a pre-shortened link under an
> unchecked key name.

## See also

`README_jams.md` · [[spclient.rs]] · [[jams_bridge.rs]] · [[mod.rs|jams/mod.rs]] · [[MOC]]
