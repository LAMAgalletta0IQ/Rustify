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

Ink. **Warm greys, deliberately** — the palette is sepia, and neutral greys
read as blue against it. Secondary text is where that showed up worst simply
because there is the most of it.

| Token | Value | Use |
| --- | --- | --- |
| `--fg` | `#f5f0e6` | Primary text (warm off-white) |
| `--fg-dim` | `#a89880` | Secondary text, idle icons |
| `--fg-faint` | `#77695a` | Section labels, placeholders |
| `--ink` | `#17110a` | Text on a light fill — primary buttons, active chips |
| `--accent` | `#1db954` | Spotify green — active states |

`--accent` is the one **cool** value left in the palette, kept because it is
the brand/functional colour marking "playing". It is intentionally the only
thing on screen that is not warm.

Glass. There is **no page background token** — the window is transparent and
Windows 11 Acrylic is the base layer (see [[tauri.conf.json]]). Surfaces are
white at a fixed set of alphas, and a panel picks one rather than inventing
its own `rgba()`:

| Token | Value | Elevation |
| --- | --- | --- |
| — | (nothing painted) | 0 · the window; Acrylic shows through |
| `--glass` | `rgba(255,241,224,.06)` | 1 · cards, chips, inputs, tab strip |
| `--glass-hover` | `rgba(255,241,224,.10)` | 1 · hover on the above |
| `--glass-raised` | `rgba(255,241,224,.085)` | 2 · player bar, popovers, hero art |
| `--glass-strong` | `rgba(255,241,224,.14)` | 3 · selected/pressed within a surface |
| `--hairline` | `rgba(255,241,224,.12)` | Borders and section rules |
| `--edge` | `inset 0 1px 0 rgba(255,241,224,.14)` | Lit top edge on elevation 2 |
| `--shadow-raised` | `0 18px 46px rgba(18,11,4,.45)` | Cast shadow on elevation 2 |
| `--blur` | `28px` | `backdrop-filter` radius |

The fills are **warm white `255,241,224`, not pure white**, and the shadow is
warm black. At these alphas the hue is not nameable on its own, but pure white
over the sepia wash turns every card faintly blue-grey and a pure-black shadow
greys the wash out under the player bar — which is exactly where the cast
shadow is most visible.

Radii `--r-sm` 10px, `--r-md` 16px, `--r-lg` 22px. Also sets
`color-scheme: dark` and the font stack
(`Segoe UI Variable Display` → `Segoe UI` → `system-ui`).

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
- `.glass` — elevation 1: `--glass` fill, `--hairline` border, blur.
- `.raised` — elevation 2: `--glass-raised` fill, `--shadow-raised` **and**
  `--edge` in one `box-shadow`, blur with `saturate(1.6)`.
- `.chip`, `.card`, `.find`, `.sec`, `.grid` — the shared component shapes.
- `.muted` — `--fg-dim`.
- `.truncate` — the standard ellipsis triple.

## Inputs / outputs / side effects

None. Imported by [[main.ts]] and extracted by Vite into a hashed CSS asset.

## Dependencies

**Imports:** none
**Imported by:** [[main.ts]]. Every component consumes its tokens and utilities.

## Notable logic / gotchas

- **Dark-only.** No `prefers-color-scheme` handling; [[tauri.conf.json]] pins
  the window `"theme": "Dark"` regardless of the system setting.

  > Until 2026-08 this also noted a `micaDark`-not-`mica` distinction. The
  > backdrop is `acrylic` now — see [[tauri.conf.json]] — which has no
  > light/dark variant of its own; darkness comes from `"theme": "Dark"` plus
  > `.veil` alone.

- **`body { background: transparent }` is load-bearing.** It is what lets the
  Acrylic backdrop reach the screen. Any opaque value here — including the
  `#0c0c10` it used to carry — hides the effect entirely, and the symptom looks
  like a broken `windowEffects` config rather than a CSS problem.
- **`--blur` does less than it looks.** `backdrop-filter` blurs only what the
  webview painted; Acrylic is composited by DWM outside it, already blurring
  what's behind the whole window before the webview paints anything. A panel
  sitting over bare backdrop gets tint and `--edge`, never an *additional*
  blur, and raising `--blur` will not change that. The elevation scale exists
  because blur cannot carry depth here.
- **Do not add a page background token back.** Elevations are relative to a
  backdrop the app does not own; a base fill would flatten all three at once.
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
