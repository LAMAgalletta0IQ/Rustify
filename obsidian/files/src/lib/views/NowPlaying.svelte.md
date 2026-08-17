---
tags: [file, frontend, ui, playback]
---
# `src/lib/views/NowPlaying.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **176 lines**

## Purpose

The full-screen now-playing overlay: large artwork, track details, and the
upcoming queue.

## Key items

### Props
`onClose: () => void` — closes the overlay in [[App.svelte]].

### State
`queue: QueueView | null`, `loading`.
`pb = $derived(store.playback)` — a shorthand alias.

### `refresh()`
Calls `api.getQueue()`, routing failures to `store.error`.

### The effect
```svelte
$effect(() => {
  pb.track?.uri;
  untrack(refresh);
});
```
Re-fetches the queue whenever the track changes. The bare expression on its own
line is the tracked dependency; `untrack` keeps `refresh`'s own state writes
from re-triggering it.

### Layout
A two-column grid: artwork and metadata on the left, a scrolling queue aside on
the right. `aside` uses `min-height: 0; overflow-y: auto` so only the queue
scrolls.

### Queue rendering
"Now playing" (from `queue.currentlyPlaying`, accented) then "Next up". Empty
queues show "Queue is empty."

## Inputs / outputs / side effects

Reads `store.playback`. Calls `get_queue` and, on click, `load_tracks`.

## Dependencies

**Imports:** `svelte` (`untrack`), [[api.ts]], [[store.svelte.ts]],
[[types.ts]]
**Imported by:** [[App.svelte]]

## Notable logic / gotchas

> **Clicking a queue entry replays the queue as an explicit track list**
> (`loadTracks(queue.queue.map(uri), t.uri)`) because **the Web API has no
> "skip to queue index" operation** — and no reorder or remove either. A source
> comment marks this. See [[queue.rs]] and [[known-limitations]].

- **Lyrics are deliberately absent.** A comment marks the reserved space and
  states the reason: Spotify's public Web API exposes no lyrics endpoint.
- **The queue reflects the *active* device**, so after transferring playback
  away this panel can show another device's queue.
- **`queue!` non-null assertion** inside the click handler is safe — the rows
  only render inside `{#if queue}`.
- Does **not** render transport controls; [[PlayerBar.svelte]] remains visible
  beneath the overlay and owns them.
- The `$effect` fires on mount too (the URI read is always the first tracked
  read), so no separate `onMount` fetch is needed.

## See also

[[queue.rs]] · [[PlayerBar.svelte]] · [[App.svelte]] · [[store.svelte.ts]] ·
[[known-limitations]] · [[frontend-views]] · [[MOC]]
