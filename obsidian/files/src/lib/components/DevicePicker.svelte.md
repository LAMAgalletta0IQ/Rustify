---
tags: [file, frontend, ui, playback]
---
# `src/lib/components/DevicePicker.svelte`

**Module:** [[frontend-components]] · **Language:** Svelte 5 · **156 lines**

## Purpose

The Spotify Connect device switcher — a popover listing every device on the
account, with transfer and a "Play here" action.

## Key items

### State
`open`, `devices: Device[]`, `loading`.

### `refresh()`
`api.listDevices()`, routing errors to `store.error`.

### `toggle()`
Flips `open` and refreshes **only when opening**, so the list is fresh each
time without polling in the background.

### `pick(d)`
Returns early if `d.id` is null (Spotify can return devices with no id, which
cannot be transfer targets — those rows render `disabled`). Otherwise
`transferPlayback(d.id, true)` then `refresh()` to update the active dot.

### Render
- Trigger `▣`, accented when `store.playback.isActiveDevice`.
- A full-screen transparent `.backdrop` button for click-away dismissal —
  simpler than a document-level listener and keeps focus handling in the DOM.
- Rows with a status dot (green when `isActive`), name and type.
- A **"Play here"** footer calling `activateThisDevice`.

## Inputs / outputs / side effects

Calls `list_devices`, `transfer_playback` (**changes playback on another
device**), and `activate_this_device`.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[types.ts]]
**Imported by:** [[PlayerBar.svelte]]

## Notable logic / gotchas

> **The two directions use different mechanisms.** Pushing playback *away* is a
> Web API call ([[connect.rs]]); pulling it *back* is a local
> `spirc.activate()` ([[player.rs]]). Both surface in this one popover. See
> [[playback-and-connect]].

- **This app appears in its own list** — it registers as a Connect device
  through `Spirc`, so `list_devices` includes `<COMPUTERNAME> (Rustify)`.
- **The list is a snapshot.** Devices appearing or disappearing while the
  popover is open are not reflected until the ⟳ button or a reopen.
- **"No devices found" is ambiguous, and once hid a real bug.** A deserialisation
  failure in [[connect.rs]] (`missing field 'isActive'`, from a two-way
  `rename_all`) produced an error, not an empty list — but the error banner was
  easy to miss and the popover's empty state read as a normal "nothing here".
  If this appears while other devices are demonstrably online, check the log for
  a `GET /me/player/devices` failure before believing the UI.
- **`isActiveDevice` is a backend heuristic**, so the trigger's accent colour
  inherits that imprecision. See [[known-limitations]].
- **The backdrop is a `<button>`**, giving keyboard dismissal and an accessible
  name for free.
- `transferPlayback(..., true)` always resumes on the target; the `play: false`
  variant that preserves pause state is supported by [[connect.rs]] but not
  exposed in the UI.

## See also

[[connect.rs]] · [[player.rs]] · [[PlayerBar.svelte]] ·
[[playback-and-connect]] · [[known-limitations]] · [[frontend-components]] ·
[[MOC]]
