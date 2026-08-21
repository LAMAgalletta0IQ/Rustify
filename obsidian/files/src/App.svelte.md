---
tags: [file, frontend, ui, entrypoint]
---
# `src/App.svelte`

**Module:** [[frontend-svelte]] · **Language:** Svelte 5

## Purpose

The application shell. Decides between four top-level states (booting, Setup
needed, logged out, logged in), owns tab navigation, renders the error banner,
and keeps [[PlayerBar.svelte]] permanently mounted.

## Key items

### State
| Name | Type | Purpose |
| --- | --- | --- |
| `tab` | `"home" \| "search" \| "library" \| "settings"` | Active tab |
| `nowPlayingOpen` | `boolean` | Now-playing overlay |
| `profileOpen` | `boolean` | Account/profile area |

### Lifecycle
```svelte
onMount(() => store.init());
onDestroy(() => store.destroy());
```
`store.init()` subscribes to backend events and attempts session restore;
`store.destroy()` unsubscribes and clears the position ticker.

### Render structure
```
{#if store.booting}          → "Starting…"
{:else if setupNeeded
       || !loggedIn}         → Setup.svelte | Login.svelte (drag strip + wctl)
{:else}
  header  (four tabs, avatar/display name opens Profile)
  banner  (store.error, dismissible)
  main    → Profile | NowPlaying | Home | Search | Library | Settings
  PlayerBar.svelte
{/if}
```
Setup and Login share one branch (and one undecorated-window drag strip)
because both are "the window before the main shell exists" — the inner
`{#if store.setupNeeded}` picks between them.

### Layout
`.shell` is an absolute flex column. Only `main` grows and scrolls
(`flex: 1; overflow-y: auto; min-height: 0`). Setup/Login live in a separate
absolute `.preauth` flex column so the 46 px custom drag strip is subtracted
from their usable viewport instead of making them one title bar too tall.

## Inputs / outputs / side effects

**Inputs:** `store.booting`, `store.setupNeeded`, `store.auth`, `store.error`.
**Side effects:** initialises/tears down the store and explicitly replaces the
Spotify integration when requested from Settings.

## Dependencies

**Imports:** `svelte` (`onMount`, `onDestroy`),
[[store.svelte.ts]], [[PlayerBar.svelte]], [[Home.svelte]], [[Login.svelte]],
[[Setup.svelte]], [[NowPlaying.svelte]], [[Search.svelte]], [[Library.svelte]],
[[Profile.svelte]], [[Settings.svelte]], and
`../src-tauri/icons/64x64.png` — the **only** import that crosses out of `src/`
into the Rust side of the tree
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
- **`setupNeeded` is checked before `loggedIn`, deliberately.** `store.init()`
  fetches `getLoginInfo()` first and returns early (skipping `restoreSession`
  entirely) when no Client ID is configured — there is nothing to restore
  until one exists. See [[store.svelte.ts]].
- `onOpenNowPlaying` **toggles** rather than sets, so the player bar acts as
  open/close.
- The error banner is app-wide; views that need bespoke error UI (notably
  [[Login.svelte]]) handle their own instead of writing to `store.error`.
- **`.ambient` and `.veil` are tints over the Acrylic backdrop, not a
  background.** Both are held well under full opacity (`.ambient` at
  `opacity: .22`, `.veil` a `rgba(14,9,4,.02)`→`.1` gradient) and `.ambient`
  has **no base colour** behind its three radial gradients. Every point added
  to either is a point of backdrop removed; a solid fill in either erases the
  effect outright.
- **`.ambient` is the sepia.** Three warm radial blobs — tobacco `#8c6239`,
  deep umber `#6b4226`, caramel `#b08046` — and nothing else in the app carries
  the hue at strength. Three *different* warm tones rather than one brown is
  deliberate: with a single hue the blobs stop reading as separate blobs and
  the 26s drift becomes invisible. `filter` is `saturate(0.85) blur(70px)`,
  softer and blurrier than a first pass at `saturate(1)` with no blur.

  > Replaced a green/indigo/magenta set (`#3d9265`, `#4a51a0`, `#8f3d6e`) and a
  > blue→purple avatar fallback (`#5c8dff`→`#b06ad9`) in 2026-08. `opacity`
  > went .3 → .45 at the same time: the sepia had to out-argue whatever hue
  > Mica pulled from the wallpaper, and at .3 a cool wallpaper cancelled it to
  > near-neutral.

  > They used to be the background: `.ambient` ended in `#0c0c10` and `.veil`
  > ran `.34`→`.66`, which was correct while the window was opaque. See
  > [[tauri.conf.json]] and [[app.css]]. Changed 2026-08.

  > **Opacity dropped again, .45 → .22, and a blur was added, when the
  > backdrop moved Mica → Acrylic** (2026-08). Acrylic shows real content
  > behind the window rather than just the wallpaper, and a flat alpha-blended
  > colour patch over arbitrarily saturated real content read as a muddy
  > clash, not a tint — the sharp gradient edges collided visibly with
  > whatever was behind. `filter: blur(70px)` removes the hard edges and
  > `saturate(0.85)` (down from `1`) keeps the blobs from being the most
  > intense colour on screen. `.veil` dropped `.04/.2` → `.02/.1` the same
  > pass, since Acrylic does some of the legibility work itself.

- **The titlebar is deliberately unpainted** — no fill, no border. Leaving it
  as bare Acrylic is what makes the custom titlebar read as part of the window
  frame rather than as a bar drawn inside it.
- **The titlebar mark imports the real bundle icon**, `64x64.png` straight out
  of `src-tauri/icons/`, rather than a copy under `src/`. One source of truth,
  so regenerating the icon set updates the titlebar too. Three things make that
  work, and all three are easy to break:
  - `src-tauri/` is inside the Vite root, so the path resolves and the file is
    emitted as a hashed asset (verified in a production build, not just dev).
  - It is therefore **same-origin**, which is what [[tauri.conf.json]]'s CSP
    `img-src 'self'` permits. A `file://` or absolute path would be blocked.
  - [[vite.config.ts]] excludes `**/src-tauri/**` from the dev watcher, so
    replacing the icon needs a restart — it will not appear on HMR.
- **`.mark .logo` needs `pointer-events: none`.** `.mark` is a
  `data-tauri-drag-region`; without it the image is a dead spot in the strip
  used to move the window. It is also sized 20px rather than the 9px the old
  accent dot used — the icon is a llama inside a disc and below ~18px it stops
  resolving into anything, reading as a worse version of the dot it replaced.

## See also

[[main.ts]] · [[store.svelte.ts]] · [[PlayerBar.svelte]] · [[Login.svelte]] ·
[[Setup.svelte]] · [[frontend-views]] · [[entry-points]] · [[frontend-svelte]] ·
[[MOC]]
