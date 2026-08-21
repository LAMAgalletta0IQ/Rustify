---
tags: [file, backend, webapi, playback, rust]
---
# `src-tauri/src/queue.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~466 lines**

> Until 2026-08 this was a 106-line pure-Web-API wrapper: `get_queue`
> (`GET /me/player/queue`) and `add_to_queue`, nothing else. It has since
> become event-first, the same shift `player.rs`'s remote handling and
> [[remote_state.rs]] went through: the Connect player-state protobuf and
> librespot's local `SetQueue` event are now the primary sources, and the
> Web API call only *enriches* what Dealer/Spirc already projected. This
> note describes the current shape.

## Purpose

Reads and appends to the playback queue, and — as of 2026-08 — builds
immediate queue projections from Dealer/Spirc events rather than waiting on
a Web API poll for every change.

> **Why this module exists separately.** It is not one of the originally
> planned backend modules. Queue state has no clean home in [[player.rs]]
> because **librespot's `Spirc` exposes no queue *reader***, and it started
> as pure Web API — so it lives on its own. The file's header comment says
> exactly this.

## Key items

### `struct QueueView`
`currently_playing: Option<TrackSummary>`, `previous: Vec<TrackSummary>`,
`queue: Vec<TrackSummary>`, `autoplay: Vec<TrackSummary>`,
`revision: Option<String>`. `previous`/`autoplay`/`revision` are all new
since the event-first rework — [[NowPlaying.svelte]]'s "Previously played"
and "Autoplay" queue sections read directly from these.

### Private wire types
`WireQueue`, `WireImage`, `WireNamed`, `WireAlbum`, `WireTrack`. `WireTrack`
marks `artists` and `duration_ms` `#[serde(default)]` because queue entries
are occasionally sparser than ordinary track objects. Only used by
`get_queue`'s Web API path now — the event-first paths below have their own
conversion from librespot's own types.

### `async fn get_queue(api, token) -> QueueView`
`GET /me/player/queue`. Populates only `currently_playing`/`queue`;
`previous`/`autoplay`/`revision` come back empty/`None` — this endpoint has
no concept of history, autoplay classification, or a revision id. Its result
is meant to be passed through `merge_metadata` as the `web` argument, not
used standalone, once an event-first `QueueView` exists.

### `fn merge_metadata(protocol: &QueueView, web: QueueView) -> QueueView`
Hydrates Dealer/Spirc-sourced tracks (which may only carry placeholder
metadata — see `from_player_event` below) with the richer public-API
representation, while **retaining** the protocol side's history, autoplay
classification, and revision, none of which the Web API has. If `protocol`
looks genuinely populated (has a revision, history, current track, or
autoplay entries), its `queue` wins outright over the Web API's; only when
`protocol` is entirely empty does the Web API's `queue` get used as a
fallback — see the gotcha below for why that check exists.

### `fn from_player_state(player: &PlayerState) -> QueueView`
Builds an **immediate** queue snapshot straight from the Connect
player-state protobuf ([[remote_state.rs]]'s Dealer subscription). Filters
out delimiter/invalid URIs via `valid_queue_uri`, classifies `next_tracks`
into `queue` vs. `autoplay` per-entry via `provided_track`'s classification,
and clamps the current track's duration up to whatever the protobuf reports
if librespot's own metadata under-reports it. This preserves changes
delivered by Dealer — including changes made by another client, a Jam, or
DJ — without waiting for a poll.

### `fn from_player_event(current, next, previous, existing) -> QueueView`
The **local** equivalent: an immediate projection from librespot's opt-in
`SetQueue` `PlayerEvent` (see [[player.rs]]'s `spawn_event_pump`, which
calls this directly and short-circuits before touching `PlaybackState`).
Existing metadata is retained by URI from `existing` (the previous
`QueueView`) so an already-known track doesn't regress to a placeholder;
brand new entries get a `placeholder()` `TrackSummary` until the normal Web
API hydration via `merge_metadata` fills them in. Classifies `queue` vs.
`autoplay` by `is_autoplay_provider`, and stamps a synthetic `revision` via
`queue_revision` (a hash of the queued URIs) since `SetQueue` carries no
revision id of its own.

### `fn provided_track(track: &ProvidedTrack) -> (TrackSummary, bool)`
Converts librespot's `ProvidedTrack` (used by both `from_player_state` and,
via `track_summary`, elsewhere) into a `TrackSummary` plus an
`is_autoplay` bool. Reads flat string metadata keys (`title`, `artist_name`
/ `artist_name:<n>` for multiple artists via a `BTreeMap` to preserve order,
`album_title`, `image_url`/`image_large_url`/`image_xlarge_url`, `duration`/
`duration_ms`, `is_explicit`, `autoplay.is_autoplay`) rather than a typed
struct, because that's the shape the Connect protobuf actually carries.

### `fn spotify_image_url(value) -> Option<String>`
Passes an already-absolute `https://` URL through; converts a bare
`spotify:image:<id>` URI into the CDN URL by hand, since the protobuf
metadata doesn't always give a ready-to-use URL the way the Web API does.

### `fn placeholder(uri) -> TrackSummary` / `fn queue_revision(next) -> String`
`placeholder` builds a minimal "Spotify track" stand-in (id parsed from the
URI's last segment, empty everything else) used until real metadata arrives.
`queue_revision` hashes the queued URIs with `DefaultHasher` to give
`from_player_event`'s output a stable-ish revision string to compare against.

### `pub(crate) fn track_summary(track: &ProvidedTrack) -> TrackSummary`
Thin wrapper discarding `provided_track`'s autoplay bool, for callers that
only need the summary (used elsewhere in the crate for jam/queue display).

### `async fn add_to_queue(api, token, uri) -> ()`
`POST /me/player/queue?uri=<encoded>`, after `validate_queue_uri` rejects
anything that isn't a canonical `spotify:track:<id>`/`spotify:episode:<id>`
URI. The URI goes in the **query string**, not a JSON body — an API quirk.

### `fn validate_queue_uri` / `fn valid_queue_uri`
`valid_queue_uri` requires exactly `spotify:(track|episode):<alphanumeric id>`
with no extra segments — rejects playlist/album/artist URIs, path traversal
attempts (`spotify:track:abc/../def`), and bare `https://open.spotify.com/...`
links. `validate_queue_uri` wraps it into an `AppError::BadRequest` for
`add_to_queue` to propagate.

### `fn is_autoplay_provider(provider) -> bool`
`provider.to_ascii_lowercase().contains("autoplay")` — the same
classification rule `from_player_state`/`from_player_event` both use to sort
a track into `queue` vs. `autoplay`.

### `fn urlencode(s: &str) -> String`
A small hand-rolled percent-encoder preserving the unreserved set
(`A-Z a-z 0-9 - _ . ~`). Written inline rather than pulling in the `urlencoding`
crate for a single call site.

### `impl From<WireTrack> for TrackSummary`
Note this coexists with an identically-named impl in [[library.rs]] — they
convert *different local* `WireTrack` types, so there is no conflict.

### Tests
Four unit tests cover: `from_player_state` splitting context/autoplay
providers and merging multi-artist metadata; `merge_metadata` **not**
restoring a stale Web API queue when the protocol side reports an
authoritative-but-empty revision; `from_player_event` deduplicating a
provider that queued the same track twice while preserving already-known
metadata; and `valid_queue_uri`'s rejection cases.

## Inputs / outputs / side effects

Network only (`get_queue`, `add_to_queue`) plus pure projection functions
(`from_player_state`, `from_player_event`, `merge_metadata`) with no I/O of
their own. `add_to_queue` **modifies the user's active playback queue**.

## Dependencies

**Imports:** `serde`, `serde_json`, `librespot::playback::player::QueueTrack`,
`librespot::protocol::player::{PlayerState, ProvidedTrack}`, [[library.rs]]
(`TrackSummary`), [[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]], [[player.rs]] (`from_player_event`),
[[remote_state.rs]] (`from_player_state`, `merge_metadata`)

## Notable logic / gotchas

- **The queue reflects the *active* device**, not necessarily this app —
  true of the Web API path. Correct while this app is active — the normal
  case — but it can show another device's queue after a transfer. Documented
  in the file header and [[known-limitations]].
- **`merge_metadata`'s "is protocol populated" check exists to stop a stale
  Web API queue from resurrecting itself.** An authoritative-but-genuinely-
  empty protocol queue (revision set, nothing queued) must not fall back to
  whatever `web.queue` still holds from before the account's queue was
  cleared — see the `authoritative_empty_revision_does_not_restore_stale_web_queue`
  test, added specifically to pin this behavior down.
- **No reorder or remove exists in the Web API.** Append is the only mutation
  possible. [[NowPlaying.svelte]] therefore implements "jump into the queue" by
  replaying it as an explicit track list via `load_tracks`.
- `POST /me/player/queue` answers `204`, handled by [[webapi.rs]].
- The hand-rolled encoder is correct for Spotify URIs (`spotify:track:...` →
  colons become `%3A`) but is not a general-purpose URL encoder.
- **Two independent local projections exist (`from_player_state`,
  `from_player_event`) because they come from two different sources** — the
  Connect cluster protobuf (Dealer, covers remote and local alike) and
  librespot's own opt-in local `SetQueue` event — and neither alone is
  guaranteed to arrive first. Both funnel through `merge_metadata` against
  the Web API result rather than replacing each other directly.

## See also

[[playback-and-connect]] · [[NowPlaying.svelte]] · [[library.rs]] ·
[[player.rs]] · [[remote_state.rs]] · [[webapi.rs]] · [[commands.rs]] ·
[[known-limitations]] · [[backend-rust]] · [[MOC]]
