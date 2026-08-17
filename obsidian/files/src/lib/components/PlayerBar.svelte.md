---
tags: [file, frontend, ui, playback]
---
# `src/lib/components/PlayerBar.svelte`

**Module:** [[frontend-components]] · **Language:** Svelte 5 · **205 lines**

## Purpose

The persistent transport bar at the bottom of the window. Rendered once by
[[App.svelte]] and never unmounted, so controls survive navigation.

## Key items

### Props
`onOpenNowPlaying: () => void` — toggles the overlay.

### Derived
- `pb = $derived(store.playback)`
- `pct = $derived(pb.durationMs > 0 ? (pb.positionMs / pb.durationMs) * 100 : 0)`
  — the guard prevents a division by zero before a track loads.

### Layout
A three-column grid: now-playing summary, centre controls, right-hand
device/volume cluster.

**Left** — artwork, title and artists; the whole block is a button opening the
now-playing view. Shows "Nothing playing" when `pb.track` is null.

**Centre** — shuffle, previous, play/pause, next, repeat, above a scrubber with
elapsed/total times from `formatMs`.

**Right** — [[DevicePicker.svelte]] and a volume slider.

### Handlers
- `onSeek` — converts slider percentage back to milliseconds.
- `onVolume` — passes `0..100`; the backend converts to librespot's scale.
- `cycleRepeat()` — **off → context → track → off**, mapped to the two
  independent booleans `set_repeat(context, track)` expects.

## Inputs / outputs / side effects

Reads `store.playback`. Calls `seek`, `set_volume`, `set_shuffle`,
`set_repeat`, `play_pause`, `next_track`, `previous_track` — all via
`store.run()`.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[types.ts]],
[[DevicePicker.svelte]]
**Imported by:** [[App.svelte]]

## Notable logic / gotchas

- **Sliders use `onchange`, not `oninput`.** `oninput` would fire a command per
  pixel of drag; `onchange` fires once on release. A deliberate trade-off:
  the position does not update live while dragging.
- **The scrubber value comes from state, not local input state.** Because
  [[store.svelte.ts]] ticks position at 1 Hz, the thumb moves on its own — and
  a drag can be visually fought by a tick mid-gesture.
- **Repeat is two booleans, not an enum.** librespot models context-repeat and
  track-repeat separately, so `cycleRepeat` synthesises a three-state cycle
  from them. [[commands.rs]] issues two Spirc calls.
- **Nothing is disabled while logged out** — the bar only renders inside
  [[App.svelte]]'s logged-in branch, so the situation cannot arise.
- **Buttons show no optimistic state.** Play/pause reflects `pb.isPlaying`,
  which only changes when the backend event arrives — correct, because a phone
  can pause too. See [[data-flow]].
- `pb.isLoading` renders "…" in place of the play/pause glyph.

## See also

[[DevicePicker.svelte]] · [[App.svelte]] · [[store.svelte.ts]] ·
[[NowPlaying.svelte]] · [[playback-and-connect]] · [[data-flow]] ·
[[frontend-components]] · [[MOC]]
