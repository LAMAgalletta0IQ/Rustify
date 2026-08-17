---
tags: [file, frontend, entrypoint, config]
---
# `index.html`

**Module:** [[project-root]] · **Language:** HTML

## Purpose

The HTML entry document. Vite's build entry point, and the page the webview
loads.

## Contents

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Rustify</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

## Key items

- **`<div id="app">`** — the mount target [[main.ts]] looks up. Renaming it
  breaks the non-null assertion there.
- **`<script type="module" src="/src/main.ts">`** — the module entry. Vite
  rewrites this at build time to point at the hashed bundle in `dist/`.
- **No `<link>` to a stylesheet** — [[app.css]] is imported from [[main.ts]] so
  it passes through the bundler and gets hashed.

## Inputs / outputs / side effects

**Input to** Vite's build ([[vite.config.ts]]).
**Output:** `dist/index.html` with rewritten asset URLs, embedded into the
binary via `frontendDist` in [[tauri.conf.json]].

## Dependencies

**References:** [[main.ts]]
**Consumed by:** Vite; then the WebView2 webview at runtime

## Notable logic / gotchas

- **It lives at the repository root, not in `src/`.** [[vite.config.ts]] sets
  no `root`, so Vite treats the project directory as its root and expects the
  entry HTML there. This is standard for the Tauri template.
- **The `<title>` is not the window title.** The window caption comes from
  `app.windows[0].title` in [[tauri.conf.json]]; this title is effectively
  invisible in a Tauri window.
- **The viewport meta is inert** — there is no mobile target and the webview
  does not pinch-zoom. Retained from the template.
- Deliberately minimal: no loading spinner, no `<noscript>`. The boot state is
  handled in [[App.svelte]] via `store.booting`.

## See also

[[main.ts]] · [[App.svelte]] · [[vite.config.ts]] · [[tauri.conf.json]] ·
[[app.css]] · [[entry-points]] · [[project-root]] · [[MOC]]
