---
tags: [file, backend, playback, rust]
---
# `src-tauri/src/player.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~804 lines**

> Until 2026-08 this was a 405-line file with two `PlaybackState` writers
> (the event pump and the remote poller) and no crossfade, DJ, or podcast
> integration. It has roughly doubled since: crossfade configuration, a
> `SetQueue` event branch that feeds [[queue.rs]] directly, a DJ Lexicon
> refill bridge, an automatic podcast-resume lookup, and the remote poller's
> demotion from primary to fallback now that [[remote_state.rs]] mirrors the
> Connect cluster event-first. This note describes the current shape.

The most intricate file in the project. Owns the librespot audio pipeline, the
Connect device identity, and the event pump that drives the entire UI.
Conceptual overview: [[playback-and-connect]] and [[state-and-events]].

## Key items

### `const MAX_VOLUME: u16 = u16::MAX`
### `fn percent_to_volume(percent: u8) -> u16`
Converts the UI's `0..=100` to librespot's `0..=65535`. **Always use this** —
see the gotcha below.

### `fn playback_config(quality, crossfade_seconds) -> PlayerConfig`
Builds `PlayerConfig` from the persisted quality/crossfade settings.
`position_update_interval` is pinned to 60s regardless of quality — besides
keeping long-form UI state fresh, it gives podcast resumption a bounded
one-minute checkpoint if the process exits without a normal pause/unload
event. Crossfade works because the player owns both decoders and overlaps
them before Rustify's own output-device/EQ sink, so the sink still sees one
continuous stream either way; `PlayerConfig`'s default keeps gapless playback
on, and crossfade must not regress that when set to 0.

### `async fn start_session(app, credentials, tokens, device_name, cache_dir, options: PlaybackOptions) -> AppResult<StartedSession>`
Builds and starts everything. Order is significant:

1. Configs: `SessionConfig`, `player_config` (via `playback_config`),
   `AudioFormat`, `MixerConfig`.
2. `ConnectConfig` — device name, `DeviceType::Computer`,
   `initial_volume: percent_to_volume(...)`, and
   `emit_set_queue_events: true` (added so local loads, remote `set_queue`
   commands, and queue additions project immediately instead of waiting for
   the next cluster push — see the `SetQueue` branch below).
3. `Cache::new(...)` — credentials + audio, capped by
   `settings.cache_limit_mb` (2 GB default, 128–8192 MB validated range).
4. `audio_backend::find(None)` → rodio → WASAPI on Windows.
5. `mixer::find(None)` → softvol mixer.
6. `Session::new`, then `Player::new` (feeding the CPAL/EQ sink via
   `audio::processing_sink`).
7. **`player.get_player_event_channel()`** — must happen here.
8. `Spirc::new(connect_config, session, credentials, player, mixer)`.
9. Spawn `spirc_task` (the Connect protocol loop).
10. `spawn_event_pump(...)`.
11. Seed `playback.volume`, `audio_quality`, `audio_quality_label` from the
    mixer/quality settings.

**No `spirc.activate()`.** It used to sit at step 11 and was removed: activating
claims active-device status, which pauses whatever is playing on the user's
other devices. Opening the app should not be a takeover. See the gotcha below.

### `struct PlaybackOptions { initial_volume_percent, cache_limit_mb, quality, crossfade_seconds }`
The settings `start_session` needs, bundled so the parameter list doesn't
keep growing one field at a time.

### `struct StartedSession { session, spirc }`
What is handed back; [[commands.rs]] pairs it with the refresher/remote/
connect-state/friends task handles to build a `SpotifySession` (see
[[state.rs]]).

### `fn spawn_event_pump(app, rx, tokens, session)`
The primary writer of `AppState.playback` for **local** playback. Consumes
librespot's `PlayerEvent` stream, mutates the snapshot, resolves metadata,
calls `refresh_position()`, and emits `playback:changed`. Unhandled variants
`continue` without emitting. Also calls `state.telemetry.observe(&event)` on
every event (see [[telemetry.rs]]).

A `PlayerEvent::SetQueue` event is handled **before** the normal match: it
projects straight into `AppState.queue` via `queue::from_player_event` and
emits `events::QUEUE`, then `continue`s — it never touches `PlaybackState`.
`context_uri` from the same event, when non-empty and not `"-"`, is written
into `pb.context_uri`.

`PlayerEvent::Paused` on an episode with `position_ms > 0` sets
`resume_report`, which the tail of the pump forwards to [[podcasts.rs]] so
Herodotus resume state stays in sync with local pauses. A `track_to_resolve`
transition can also trigger `spawn_dj_refill` (ended-track was from an active
DJ window) or `spawn_resume_lookup` (new track is an episode) — see below.

### `fn spawn_dj_refill(app, session, ended_uri)`
Best-effort compatibility bridge until librespot itself understands
Lexicon/`hm://` dynamic contexts. Fires only for a track that ended from the
active DJ window ([[spotify/dj.rs]]) and only when the observed queue is
getting low (`remaining < 8`); re-resolves the DJ context, diffs it against
what's already queued via `spotify::dj_refill_uris`, and pushes any new URIs
onto the Spirc queue directly. Guarded by `begin_dj_refill()`/
`finish_dj_refill()` so overlapping refills can't race. Failures only log —
they never interrupt playback.

### `fn spawn_resume_lookup(app, session, uri)`
Fires when the event pump resolves a **new** track that's an episode: looks
up Herodotus resume state via [[podcasts.rs]], and if there's an unfinished
position beyond 2s, seeks there automatically — but only if that episode is
still the current track on this device by the time the lookup returns
(`still_current` check against `is_active_device`/`track.uri`/
`position_ms <= 2_000`), so a fast track-skip during the lookup can't cause a
seek on the wrong track.

### `const REMOTE_POLL: Duration = 5s`
### `fn spawn_remote_poller(app, tokens) -> JoinHandle<()>`
**No longer the primary remote-state source** — see the gotcha below.
Mirrors playback from **other** Connect devices by polling `GET /me/player`,
gated on `ActiveDeviceSignal` (not the `PlaybackState` copy) rather than
sleeping unconditionally: it `select!`s on either the poll interval or the
active-device watch channel changing, so becoming active suspends polling
immediately instead of waiting out the current tick, and the very first
iteration polls before the first sleep so opening the app while another
device plays shows it immediately.

### `async fn apply_remote(state, Option<RemotePlayback>) -> bool`
Folds a `/me/player` response into `PlaybackState`, returning whether
anything changed (compared via `remote_identity`, a plain-data snapshot of
the fields that matter for change detection). `None` (HTTP 204) means
nothing is playing anywhere and clears `track`/`context_uri`/`active_device`
along with the transport flags. Prefers the album's images for cover art,
falling back to the item's own (episodes carry theirs directly).

### `struct RemoteIdentity` / `fn remote_identity(playback) -> RemoteIdentity`
A comparison snapshot (`playing`, `loading`, `position`, `duration`, `track`,
`context`, `volume`, `shuffle`, both repeat flags, `active_device`) taken
before and after applying a remote poll, so `apply_remote` can report
"nothing changed" and the poller can skip emitting a redundant
`playback:changed`.

### `async fn fetch_track_info(api, token, uri) -> TrackInfo`
Branches on `uri.item_type()`: `"episode"` → `/v1/episodes/{id}`, everything
else → `/v1/tracks/{id}`.

### `fn pick_cover(images) -> Option<String>`
Chooses the image closest to **300px** rather than the largest, so a low-end
CPU is not decoding a 640px JPEG for a 52px thumbnail.

### `async fn snapshot(state) -> PlaybackState`
Backs the `get_playback` command and reconnect/initial-load paths. Takes a
**write** lock rather than a read one, because it calls `refresh_position()`
first — a snapshot taken mid-track must not report the position from the
last event.

## Inputs / outputs / side effects

- **Audio output** via rodio/WASAPI — the app's core side effect.
- **Network:** librespot's Spotify connection; Web API metadata lookups;
  `GET /me/player` polling (fallback path only).
- **Filesystem:** librespot cache under `<app data>/cache`.
- **Spawns** long-lived tasks: `spirc_task`, the event pump, and the remote
  poller — [[remote_state.rs]]'s `connect_state_task` and [[friends.rs]]'s
  `friends_task` are spawned alongside these by [[commands.rs]], not by this
  file, but land in the same `SpotifySession`.
- **Emits** `playback:changed` and `queue:changed` to the webview.

## Dependencies

**Imports:** `librespot::{connect, core, playback}`, `tauri`, `tokio`, `serde`,
[[state.rs]], [[connect.rs]], [[webapi.rs]], [[error.rs]], [[queue.rs]],
[[podcasts.rs]], `crate::spotify` (DJ refill)
**Imported by:** [[commands.rs]]

## Notable logic / gotchas

> ### 1. `get_player_event_channel()` before the move
> `Spirc::new` **takes ownership** of `player`. The event channel must be
> obtained beforehand or there is no way to observe playback at all. This
> single ordering constraint is what makes the whole UI work.

> ### 2. `initial_volume` is raw, not a percentage
> The field is on the `0..=65535` scale; librespot's own default is
> `u16::MAX / 2`. Passing `50` gives ~0.08% volume — audible silence that looks
> exactly like a broken audio backend. librespot's doc comment reads
> `(default: 50%)`, describing *intent*, not the literal. A source comment
> marks this.

- **Metadata comes from the Web API, not `AudioItem`.** librespot's covers are
  raw file IDs needing URL construction; the Web API returns ready-to-use CDN
  URLs and a documented shape. Cost: one HTTP request per *new* track, cached
  by URI in a `HashMap` for the session.
- **A metadata failure is non-fatal** — logs a warning and leaves the previous
  track displayed rather than blanking the bar.
- **The pump reads the token through `TokenStore`**, so refreshes are picked up
  automatically. Passing a `String` here was a real bug.

> ### 3. Registering as a device is not activating it
> `Spirc::activate` claims active-device status, which by protocol pauses
> playback elsewhere. Calling it in `start_session` meant simply **opening the
> app stopped music on the user's phone**. It now happens only on an explicit
> load or `activate_this_device`, both in [[commands.rs]], which check
> `is_active_device` first to avoid librespot's
> `SpircCommand::Activate will be ignored while already active`.

> ### 4. Not every event carries a position
> `VolumeChanged`, `ShuffleChanged` and `RepeatChanged` report none, yet the
> whole snapshot is sent on every event — so they shipped the last reported
> position, normally 0, and the UI clock jumped to 0:00 while audio played on.
> Positions now go through `set_position()` (anchor) and `refresh_position()`
> (recompute), never a bare assignment. See [[state.rs]].

> ### 5. The remote poller was demoted (2026-08)
> Until [[remote_state.rs]] existed, `spawn_remote_poller` was the *only*
> source of remote-device state and ran on a plain 5s sleep loop regardless
> of activity. It's now a fallback for whatever the Dealer-driven cluster
> mirror doesn't cover, gated on `active_rx` via `select!` so it suspends the
> instant this device becomes active rather than on the next tick boundary.
> Both writers still target the same `PlaybackState`/`QueueView` fields — see
> [[remote_state.rs]]'s "two writers, reconciled on purpose" note for how
> that's kept from fighting itself.

- **`is_active_device` is inferred** from `Playing`/`Paused`/`Loading`/
  `SessionConnected` vs. `SessionDisconnected`; librespot exposes no explicit
  flag. `ActiveDeviceSignal::set` is the canonical write; `PlaybackState`'s
  copy of the same fact is a mirror for the frontend snapshot (see
  [[state.rs]]).
- **The poller is the app's only recurring network timer besides Dealer
  subscriptions**, and it is gated on *not* being the active device. See
  [[rate-limiting]] before adding another.
- **`spirc_task` must keep running.** Drop it and the device disappears from
  Spotify Connect.
- **The cache cap is explicit and persisted**; librespot would otherwise grow
  unbounded. Changes apply when the next session constructs its cache.
- The `_ =>` arm `drop(pb)`s the write guard before `continue`, so no snapshot
  is emitted for events the UI does not render.
- **DJ refill and resume lookup are both fire-and-forget `tauri::async_runtime::spawn`
  tasks**, not awaited by the event pump — a slow or failing DJ refresh or
  podcast lookup can never stall the pump loop that everything else depends
  on.

## See also

[[playback-and-connect]] · [[state-and-events]] · [[data-flow]] ·
[[commands.rs]] · [[state.rs]] · [[remote_state.rs]] · [[queue.rs]] ·
[[podcasts.rs]] · [[spotify/dj.rs]] · [[webapi.rs]] · [[auth.rs]] ·
[[known-limitations]] · [[backend-rust]] · [[MOC]]
