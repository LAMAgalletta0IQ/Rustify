---
tags: [file, backend, webapi, playback, rust]
---
# `src-tauri/src/connect.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **94 lines**

Lists Connect devices, moves playback between them, and reads what is playing
account-wide.

## Key items

### `struct Device`
`id: Option<String>`, `name`, `device_type` (JSON `type` via
`#[serde(rename = "type")]`), `is_active`, `is_restricted`, `volume_percent`.

`id` is `Option` because Spotify may return a device with no id — such a device
cannot be a transfer target, which is why [[SpotifyConnectMenu.svelte]] disables
those rows.

Carries a **one-sided** rename: `#[serde(rename_all(serialize = "camelCase"))]`.
See the gotcha below.

### `struct RemotePlayback` and friends
The `/me/player` payload: `device`, `is_playing`, `progress_ms`, `item`,
`repeat_state`, `shuffle_state`. `RemoteItem` covers a track; an episode omits
`artists`/`album`, hence the `#[serde(default)]`s.

### `async fn current_playback(api, token) -> Option<RemotePlayback>`
`GET /me/player`. `Ok(None)` on HTTP 204 — nothing playing anywhere. Driven by
`spawn_remote_poller` in [[player.rs]]; the only way to see playback on another
device, since `PlayerEvent`s describe only this app's own audio.

### `async fn list_devices(api, token) -> Vec<Device>`
`GET /me/player/devices`, unwrapping the `{devices: [...]}` envelope. Includes
**this app**, which registers itself through `Spirc` in [[player.rs]].

### `async fn transfer_playback(api, token, device_id, play) -> ()`
`PUT /me/player` with `{"device_ids": [id], "play": bool}`. `play: true`
resumes on the target; `false` preserves the current play/pause state.

## Inputs / outputs / side effects

Network only. `transfer_playback` **changes playback on another device** —
audible on hardware the user may not be looking at.

## Dependencies

**Imports:** `serde`, `serde_json::json`, [[webapi.rs]], [[error.rs]]
**Imported by:** [[commands.rs]]

## Notable logic / gotchas

- **`rename_all` applies to both directions — that was a bug.** `Device` derives
  `Serialize` *and* `Deserialize`. With a plain
  `#[serde(rename_all = "camelCase")]`, deserialisation expected `isActive`
  while Spotify sends `is_active`, so **every** device list failed with
  `missing field 'isActive'` and [[SpotifyConnectMenu.svelte]] rendered "No devices
  found" — a parse failure wearing a friendly empty state. Restricting the
  rename to `serialize` keeps the wire names inbound and camelCase outbound.
  Any struct here that is both read from Spotify and sent to the webview needs
  the same treatment.
- **This module is Web API, not librespot — deliberately.** `Spirc` controls
  *this* device; commanding a different one is a server-side operation. See
  [[playback-and-connect]] for the full reasoning.
- **Transferring away is not the inverse of pulling back.** Pulling playback
  back uses `activate_this_device` → `spirc.activate()` in [[commands.rs]],
  not a transfer. The device picker therefore mixes both mechanisms.
- **`device_ids` is an array** in the request even though Spotify only honours
  one entry.
- The response to `PUT /me/player` is `204 No Content`, handled centrally by
  [[webapi.rs]].
- `is_restricted` is surfaced but not currently acted on; a restricted device
  may reject commands.

## See also

[[playback-and-connect]] · [[SpotifyConnectMenu.svelte]] · [[commands.rs]] ·
[[player.rs]] · [[webapi.rs]] · [[backend-rust]] · [[MOC]]
