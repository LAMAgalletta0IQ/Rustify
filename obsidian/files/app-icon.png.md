---
tags: [file, asset, config]
---
# `app-icon.png`

**Module:** [[project-root]] · **Type:** PNG image, 1024×1024, ~28 KB

## Purpose

The single source image for the entire application icon set. Everything in
[[icons]] is generated from this file.

## What it depicts

A placeholder generated programmatically with .NET `System.Drawing`:

- Near-black background — `#0b0b0d`, matching `--bg` in [[app.css]].
- A green disc inset 64 px — `#1db954`, matching `--accent`.
- A dark eighth-note glyph — two note heads, two stems, and a connecting beam,
  in `#04120a`.
- Antialiased, with round line caps.

It is intentionally a stand-in: recognisable in a taskbar and consistent with
the app palette, but not designed artwork.

## How it is consumed

```powershell
npm run tauri icon app-icon.png
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
- **Square, ideally 1024×1024.** Smaller sources produce blurry large tiles;
  non-square inputs are distorted.
- Not listed in [[.gitignore]] — it is a source asset and belongs in version
  control, as does its generated output (Tauri needs `icon.ico` present).
- Regeneration also emits unused iOS and Android assets; see [[icons]].

## See also

[[icons]] · [[build.rs]] · [[tauri.conf.json]] · [[app.css]] ·
[[project-root]] · [[MOC]]
