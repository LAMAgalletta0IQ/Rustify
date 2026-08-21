---
tags: [file, frontend, ui, library]
---
# `src/lib/components/TrackList.svelte`

Reusable playable track rows with duration, queue append, and Liked Songs
state. A supplied context URI uses `load_context(context, track)`; otherwise
the visible URI list is loaded as an explicit context.

Saved state uses the generic `/me/library/contains` adapter and chunks visible
IDs at 40. Each heart is optimistic, `aria-pressed`, and guarded by a per-track
pending map so repeated clicks cannot race. A failed mutation rolls the exact
row back and reports the typed backend message. Focus-visible hearts/queue
buttons remain visible even without pointer hover.

## See also

[[library.rs]] · [[api.ts]] · [[webapi.rs]] · [[MOC]]
