---
tags: [file, config, build, frontend]
---
# `vite.config.ts`

**Module:** [[project-root]] · **Language:** TypeScript

## Purpose

Vite configuration, tuned specifically for running inside Tauri rather than a
browser.

## Key items

### `plugins: [svelte()]`
Compiles `.svelte` files, using the preprocessor from [[svelte.config.js]].

### `clearScreen: false`
Stops Vite wiping the terminal, so Rust compiler output stays visible during
`tauri dev` — both processes share one console.

### `server`
```ts
port: 1420,
strictPort: true,
watch: { ignored: ["**/src-tauri/**"] }
```
- **1420 must match `devUrl`** in [[tauri.conf.json]].
- **`strictPort: true`** makes a busy port fail loudly instead of silently
  moving to 1421 and leaving Tauri pointed at nothing.
- **Ignoring `src-tauri/`** prevents Rust rebuild artifacts from retriggering
  the frontend watcher in a loop.

### `build`
```ts
target: "chrome110",
minify: "esbuild",
sourcemap: false
```

> **`target: "chrome110"` is the interesting one.** The runtime is WebView2 —
> evergreen Chromium — not an arbitrary browser. Targeting it directly means
> little transpilation and no legacy polyfills, keeping the bundle at ~64 KB
> (23 KB gzipped). In a normal web project this would be unsafe.

`sourcemap: false` trims build output; debugging happens in dev mode.

## Inputs / outputs / side effects

**Input:** [[index.html]] as the entry, which pulls in [[main.ts]].
**Output:** `dist/` — excluded from this vault (see [[excluded]]) and embedded
into the binary at compile time.

## Dependencies

**Imports:** `vite`, `@sveltejs/vite-plugin-svelte`
**Coupled to:** [[tauri.conf.json]] (port), [[svelte.config.js]] (preprocessor)

## Notable logic / gotchas

- **The port coupling is the classic dev-mode failure.** If 1420 is occupied,
  `strictPort` aborts Vite and `tauri dev` shows a blank window.
- **`dist/` is embedded at compile time.** Running `vite build` after cargo has
  already compiled does not change the binary. See [[build-and-config]].
- **`root` is not set**, so it defaults to the project directory — which is why
  [[index.html]] sits at the repository root rather than under `src/`.
- No `envPrefix` or define block; the frontend reads no environment variables.

## See also

[[tauri.conf.json]] · [[svelte.config.js]] · [[index.html]] · [[main.ts]] ·
[[package.json]] · [[build-and-config]] · [[project-root]] · [[MOC]]
