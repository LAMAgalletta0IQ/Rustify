---
tags: [file, frontend, entrypoint]
---
# `src/main.ts`

**Module:** [[frontend-svelte]] · **Language:** TypeScript · **7 lines**

## Purpose

The frontend entry point. Mounts the Svelte application and pulls in global
styles.

## Contents

```ts
import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

const app = mount(App, { target: document.getElementById("app")! });

export default app;
```

## Key items

- **`mount(App, { target })`** — the Svelte 5 API. Replaces Svelte 4's
  `new App({ target })`; using the class form here would be a runtime error.
- **`import "./app.css"`** — Vite extracts this into a separate CSS asset at
  build time. It is imported here rather than linked from [[index.html]] so it
  passes through the bundler.
- **`document.getElementById("app")!`** — the non-null assertion is safe
  because [[index.html]] always contains `<div id="app">`.

## Inputs / outputs / side effects

**Side effect:** renders the entire UI into the DOM. Exports the app instance,
which nothing consumes — kept for HMR compatibility.

## Dependencies

**Imports:** `svelte`, [[App.svelte]], [[app.css]]
**Imported by:** [[index.html]] via `<script type="module" src="/src/main.ts">`

## Notable logic / gotchas

- This runs on **every** webview load, in both dev and release. In dev it is
  served by Vite; in release it is part of the bundle embedded in the binary.
- No error boundary. If [[App.svelte]] throws during mount the page is blank —
  the boot-failure path is handled inside [[store.svelte.ts]] instead, whose
  `finally` block guarantees `booting` clears.

## See also

[[App.svelte]] · [[app.css]] · [[index.html]] · [[entry-points]] ·
[[frontend-svelte]] · [[MOC]]
