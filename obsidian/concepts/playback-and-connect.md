---
tags: [concept, playback]
---
# Playback and Spotify Connect

Why `Spirc` is the central primitive, and why device switching goes through
HTTP instead.

## `Spirc`, not `Player`

librespot offers two levels:

| | `Player` | `Spirc` |
| --- | --- | --- |
| Plays audio | ✅ | ✅ (owns a `Player`) |
| Appears as a Connect device | ❌ | ✅ |
| Accepts remote commands | ❌ | ✅ |
| Tracks shuffle/repeat/context | ❌ | ✅ |

This app builds a `Player`, hands it to `Spirc`, and then **only ever talks to
`Spirc`** ([[player.rs]]).

The reason is state coherence. If local buttons called `Player` directly while
a phone drove `Spirc`, the two would disagree about what is playing. Routing
everything through `Spirc` means local clicks and remote commands take the
identical path, so divergence is structurally impossible.

Cost: transport control is asynchronous. `spirc.play()` returns immediately;
the UI updates when the resulting `PlayerEvent` arrives. See [[data-flow]].

## Construction order

From `start_session` in [[player.rs]]:

```rust
let sink_builder  = audio_backend::find(None)?;   // rodio → WASAPI on Windows
let mixer_builder = mixer::find(None)?;           // softvol
let session = Session::new(session_config, Some(cache));
let mixer   = mixer_builder(mixer_config)?;
let player  = Player::new(config, session, mixer.get_soft_volume(), sink_builder);
let event_rx = player.get_player_event_channel();   // ← BEFORE the move
let (spirc, spirc_task) = Spirc::new(connect_config, session, creds, player, mixer)?;
```

**`get_player_event_channel()` must be called before `player` moves into
`Spirc`.** After the move the handle is gone and there is no way to observe
playback. This single ordering constraint is what makes the whole UI work.

`spirc_task` is then spawned and must run for the lifetime of the login — it
*is* the Connect protocol loop. Drop it and the device vanishes from Spotify.

## Being a Connect device (inbound)

`ConnectConfig` sets the identity:

| Field | Value |
| --- | --- |
| `name` | `<COMPUTERNAME> (spotify-rust)` |
| `device_type` | `DeviceType::Computer` |
| `initial_volume` | `percent_to_volume(50)` — **raw 0..=65535 scale** |

Registration happens over Spotify's dealer/websocket once logged in, **not**
mDNS. This is why librespot's `with-libmdns` default feature is disabled in
[[Cargo.toml]] — zeroconf discovery only matters for the "log in from another
device" flow, which this app does not use.

`spirc.activate()` claims active-device status at startup.

> ### The `initial_volume` trap
> The field is on the **raw `0..=65535`** scale, not a percentage — librespot's
> own default is `u16::MAX / 2`. Passing `50` yields ~0.08% volume: audible
> silence that looks exactly like a broken audio backend. The docs comment
> reads `(default: 50%)`, describing intent rather than the literal. Always
> convert with `percent_to_volume`.

## Switching to another device (outbound)

Here the app stops using librespot and uses HTTP ([[connect.rs]]):

- `GET /v1/me/player/devices` — list devices, including this app.
- `PUT /v1/me/player` with `{device_ids: [...], play: bool}` — transfer.

**Why not librespot?** `Spirc` controls *this* device. Commanding a different
device is a server-side operation, and the Web API is the documented way to do
it. `Spirc::transfer` exists but handles the inbound direction — accepting a
transfer *to* us.

Pulling playback back is the opposite: `activate_this_device` calls
`spirc.activate()` locally rather than issuing a transfer.

So the device picker in [[DevicePicker.svelte]] mixes both mechanisms: HTTP to
list and push away, librespot to pull back.

## Loading content

`LoadRequest` has two constructors, both in [[commands.rs]]:

| Command | Constructor | Used for |
| --- | --- | --- |
| `load_context` | `from_context_uri` | Playlist, album, artist, Liked Songs |
| `load_tracks` | `from_tracks` | Search results, queue entries |

Both set `start_playing: true`. **The default is `false`** — relying on the
default and calling `play()` afterwards is a needless second round trip and a
race.

`playing_track: Some(PlayingTrack::Uri(...))` starts a context at a chosen
track. Without it, clicking track 5 of a playlist starts at track 1.

Loading a bare track URI as a context plays that one track and stops — this was
a real bug. Always prefer the container.

## Queue

Read via `GET /v1/me/player/queue` ([[queue.rs]]); append via
`POST /v1/me/player/queue`. `Spirc` exposes no queue *reader*, which is why
queue handling is Web API rather than librespot.

Two caveats: the endpoint reflects the **active** device's queue (accurate
while this app is active, the normal case), and there is no reorder or remove
operation in the API at all. See [[known-limitations]].

## Volume

Three representations, converted at the boundaries:

```
UI slider 0..100  ──percent_to_volume──►  librespot 0..65535
UI display        ◄──volumeToPercent────  PlaybackState.volume
```

`percent_to_volume` lives in [[player.rs]]; `volumeToPercent` in [[types.ts]].
Volume is seeded from `mixer.volume()` at startup so the slider is correct
before the first `VolumeChanged` event.

## See also

[[architecture]] · [[data-flow]] · [[state-and-events]] · [[player.rs]] ·
[[connect.rs]] · [[queue.rs]] · [[commands.rs]] · [[DevicePicker.svelte]] ·
[[known-limitations]] · [[MOC]]
