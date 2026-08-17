---
tags: [file, backend, webapi, playback, rust]
---
# `src-tauri/src/connect.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **47 lines**

The smallest feature module. Lists Connect devices and moves playback between
them.

## Key items

### `struct Device`
`id: Option<String>`, `name`, `device_type` (serialised from JSON `type` via
`#[serde(rename = "type")]`), `is_active`, `is_restricted`, `volume_percent`.

`id` is `Option` because Spotify may return a device with no id — such a device
cannot be a transfer target, which is why [[DevicePicker.svelte]] disables
those rows.

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

[[playback-and-connect]] · [[DevicePicker.svelte]] · [[commands.rs]] ·
[[player.rs]] · [[webapi.rs]] · [[backend-rust]] · [[MOC]]
