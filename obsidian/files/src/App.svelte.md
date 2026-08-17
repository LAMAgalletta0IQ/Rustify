---
tags: [file, frontend, ui, entrypoint]
---
# `src/App.svelte`

**Module:** [[frontend-svelte]] · **Language:** Svelte 5 · **126 lines**

## Purpose

The application shell. Decides between three top-level states (booting, logged
out, logged in), owns tab navigation, renders the error banner, and keeps
[[PlayerBar.svelte]] permanently mounted.

## Key items

### State
| Name | Type | Purpose |
| --- | --- | --- |
| `tab` | `"home" \| "search"` | Active tab |
| `nowPlayingOpen` | `boolean` | Now-playing overlay |

### Lifecycle
```svelte
onMount(() => store.init());
onDestroy(() => store.destroy());
```
`store.init()` subscribes to backend events and attempts session restore;
`store.destroy()` unsubscribes and clears the position ticker.

### Render structure
```
{#if store.booting}        → "Starting…"
{:else if !loggedIn}       → Login.svelte
{:else}
  header  (tabs, avatar, display name, log out)
  banner  (store.error, dismissible)
  main    → NowPlaying | Home | Search
  PlayerBar.svelte
{/if}
```

### Layout
`.shell` is a CSS grid with rows `auto auto 1fr auto` — header, banner, scrolling
content, player bar. Only `main` scrolls (`overflow-y: auto; min-height: 0`);
the `min-height: 0` is required or the grid row refuses to shrink and the page
scrolls as a whole.

## Inputs / outputs / side effects

**Inputs:** `store.booting`, `store.auth`, `store.error`.
**Side effects:** initialises and tears down the store; triggers `logout`.

## Dependencies

**Imports:** `svelte` (`onMount`, `onDestroy`), [[api.ts]],
[[store.svelte.ts]], [[PlayerBar.svelte]], [[Home.svelte]], [[Login.svelte]],
[[NowPlaying.svelte]], [[Search.svelte]]
**Imported by:** [[main.ts]]

## Notable logic / gotchas

- **`NowPlaying` replaces tab content rather than stacking over it.** The
  `{#if nowPlayingOpen}` branch comes first inside `main`, so opening it hides
  Home/Search entirely. Returning restores the previous tab, since `tab` is
  untouched.
- **`PlayerBar` sits outside the conditional** and never unmounts, so playback
  controls and its device popover survive navigation.
- **The boot gate is the only guard against a flash of the login screen.**
  Without `store.booting`, a successful silent restore would still render
  [[Login.svelte]] for a frame.
- `onOpenNowPlaying` **toggles** rather than sets, so the player bar acts as
  open/close.
- The error banner is app-wide; views that need bespoke error UI (notably
  [[Login.svelte]]) handle their own instead of writing to `store.error`.

## See also

[[main.ts]] · [[store.svelte.ts]] · [[PlayerBar.svelte]] · [[Login.svelte]] ·
[[frontend-views]] · [[entry-points]] · [[frontend-svelte]] · [[MOC]]
