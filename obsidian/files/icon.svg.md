---
tags: [file, asset, config]
---
# `icon.svg`

**Module:** [[project-root]] · **Type:** SVG image, 500×500 viewBox

## Purpose

The single source image for the entire application icon set. Everything in
[[icons]] is generated from this file.

> Until 2026-08 the source was `app-icon.png`, a placeholder generated
> programmatically with .NET `System.Drawing` — a green disc with a dark
> eighth-note glyph on a near-black field. `icon.svg` replaced it with
> hand-authored vector artwork; the file was regenerated with
> `npx tauri icon icon.svg`, which overwrote every file under
> [[icons]] (Windows, and the unused macOS/iOS/Android sets) in place.

## What it depicts

A 3D-beveled gear (the "settings"/mechanism motif) merged with a vinyl record
visible through the gear's center hole:

- A dark vinyl disc with concentric grooves and two anisotropic glare
  gradients, sitting underneath the gear.
- Rotated sound-wave arcs in warm off-white, in the gear's lower-left, with a
  drop shadow.
- An 8-tooth gear body in a warm off-white gradient (`#F4EFE5` → `#D6CFBF`),
  built from a masked base circle plus four rotated rounded rectangles, with
  an inner-bevel filter (`feGaussianBlur`/`feOffset`/`feComposite` chain) for
  highlight/shadow/glow.
- A tonearm extension merged into the top-right tooth, housing a green pill.
- A center record label (the gear's hub) with an engraved ring detail.
- Two green (`#258C46`) indicator details — the center dot and the tonearm
  pill — each with an inner-shadow filter for depth.

The warm off-white palette matches `--fg`/`--glass` in [[app.css]]; the green
matches `--accent`.

## How it is consumed

```powershell
npx tauri icon icon.svg
```

Regenerates all 57 files under `src-tauri/icons/`, including `icon.ico` — the
one file [[build.rs]] embeds as a Windows resource.

**This file is not referenced by the build.** It is the input to a manual
regeneration step; only the generated output matters at compile time.

## Inputs / outputs / side effects

**Output:** the icon set described in [[icons]].

## Dependencies

**Consumed by:** `tauri icon` (the CLI from [[package.json]])
**Produces:** [[icons]]

## Notable logic / gotchas

- **Replacing the artwork requires re-running `tauri icon`.** Overwriting this
  file alone changes nothing — the build reads `icons/icon.ico`.
- **SVG or square PNG, ideally ~1024×1024 for a raster source.** The CLI
  accepts either; a non-square input is distorted.
- Not listed in [[.gitignore]] — it is a source asset and belongs in version
  control, as does its generated output (Tauri needs `icon.ico` present).
- Regeneration also emits unused iOS and Android assets; see [[icons]].

## See also

[[icons]] · [[build.rs]] · [[tauri.conf.json]] · [[app.css]] ·
[[project-root]] · [[MOC]]
