---
tags: [file, frontend, ui]
---
# `src/app.css`

**Module:** [[frontend-svelte]] · **Language:** CSS

## Purpose

Global stylesheet: design tokens, resets, and the few shared utility classes.
Everything else is scoped inside components.

## Key items

### Design tokens (`:root`)
| Token | Value | Use |
| --- | --- | --- |
| `--bg` | `#0b0b0d` | Page background |
| `--bg-elev` | `#141418` | Cards, header, player bar |
| `--bg-elev-2` | `#1c1c22` | Hover, inputs, popovers |
| `--fg` | `#f2f2f4` | Primary text |
| `--fg-dim` | `#a0a0aa` | Secondary text, idle icons |
| `--accent` | `#1db954` | Spotify green — active states |
| `--border` | `#26262e` | Dividers |
| `--radius` | `8px` | Standard corner radius |

Also sets `color-scheme: dark` and the font stack
(`Segoe UI Variable` → `Segoe UI` → `system-ui`).

### Resets
`* { box-sizing: border-box }`; `html, body, #app` at `height: 100%`,
`margin: 0`, **`overflow: hidden`** — the shell manages its own scrolling
regions, so the page itself must never scroll.

### Element defaults
`button` is stripped to a transparent, inheriting control with
`cursor: pointer`; `:disabled` drops to `opacity: .45`. `input` gets the
elevated background and an accent focus border.

### Scrollbars
`::-webkit-scrollbar` styled 10px with a `#33333d` thumb and transparent track,
so panes do not show default light scrollbars against the dark UI.

### Utilities
- `.btn-primary` — accent pill button (Play, Log in).
- `.muted` — `--fg-dim`.
- `.truncate` — the standard ellipsis triple.

## Inputs / outputs / side effects

None. Imported by [[main.ts]] and extracted by Vite into a hashed CSS asset.

## Dependencies

**Imports:** none
**Imported by:** [[main.ts]]. Every component consumes its tokens and utilities.

## Notable logic / gotchas

- **Dark-only.** No `prefers-color-scheme` handling; [[tauri.conf.json]] also
  pins the window `"theme": "Dark"`, so the two agree.
- **`overflow: hidden` on `#app` is load-bearing.** [[App.svelte]]'s grid
  relies on `main` being the only scrollable region; removing it produces a
  double scrollbar.
- **`-webkit-` scrollbar styling is safe here** because the target is
  WebView2 (Chromium) exclusively — no cross-browser concern.
- Utility classes are intentionally minimal; component-scoped CSS is preferred
  over a global utility system.

## See also

[[main.ts]] · [[App.svelte]] · [[tauri.conf.json]] · [[frontend-svelte]] ·
[[MOC]]
