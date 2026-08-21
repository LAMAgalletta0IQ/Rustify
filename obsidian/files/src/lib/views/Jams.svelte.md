---
tags: [file, frontend, ui, jam]
---
# `src/lib/views/Jams.svelte`

**Module:** [[frontend-views]] · **Language:** Svelte 5 · **~270 lines**

## Purpose

Full jam lifecycle: create, join by link/id, leave, add current track,
toggle participant queue control, kick a member, end the session (owner
only), and a live member/queue list driven by dealer events. Reached via a
Jam icon button in the player bar rather than a top-level nav tab (see
[[known-limitations]] on navigation), since a jam is a contextual player/
social action, not a standing destination.

## Key items

### `refresh()` / dealer event listener
`onMount` calls `refresh()` once, then subscribes to `EVENT_JAMS`; any
dealer push (member joined/left, queue changed, permissions changed) just
triggers another `refresh()` rather than trying to apply the push payload's
shape locally — the backend snapshot is always the source of truth.

### `onCopyInvite()`
Copies `joinUrl ?? joinUri` to the clipboard.

## Inputs / outputs / side effects

Calls `getJamStatus`, `createJam`, `joinJam`, `leaveJam`, `addTrackToJam`,
`setJamQueueControl`, `kickJamMember`, `endJam`. Listens for `jams:changed`.

## Dependencies

**Imports:** [[api.ts]], [[store.svelte.ts]], [[types.ts]]
**Imported by:** [[App.svelte]] (via the player bar's Jam button, not a nav route)

## Notable logic / gotchas

> ### Raw diagnostics removed (2026-08)
> This page used to read like a developer test harness: a live
> "spclient endpoints: N captured · pathfinder hashes: N captured" counter,
> the raw session id in a monospace chip, and a "Live events" panel that
> `JSON.stringify`'d every dealer push onto the screen. All three were
> removed from normal UI — none were consumed by anything except a human
> reading them, so nothing downstream changed. The one diagnostic kept is
> the "no endpoints configured" hint, which is genuinely actionable (a local
> config problem) and now worded in plain language instead of naming
> `jams.toml`/`[spclient_endpoints]`.

## See also

`README_jams.md` · [[jams_bridge.rs]] · [[api.ts]] · [[App.svelte]] ·
[[frontend-views]] · [[MOC]]
