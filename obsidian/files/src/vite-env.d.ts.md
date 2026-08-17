---
tags: [file, frontend, config]
---
# `src/vite-env.d.ts`

**Module:** [[frontend-svelte]] · **Language:** TypeScript declaration · **2 lines**

## Purpose

Ambient type references so TypeScript understands Svelte components and Vite's
runtime additions.

## Contents

```ts
/// <reference types="svelte" />
/// <reference types="vite/client" />
```

## Key items

- **`svelte`** — teaches TypeScript what a `.svelte` import resolves to.
  Without it, `import App from "./App.svelte"` is an unresolved-module error.
- **`vite/client`** — declares `import.meta.env`, and the module shapes for
  asset imports such as `?url` and `?raw`.

## Inputs / outputs / side effects

None. Emits no JavaScript; consumed only by the type checker.

## Dependencies

**Imports:** ambient types from `svelte` and `vite`
**Imported by:** nothing explicitly — picked up via the `include` glob in
[[tsconfig.json]]

## Notable logic / gotchas

- **Deleting this file breaks the build**, not just the editor: `npm run build`
  runs `svelte-check` first ([[package.json]]), so unresolved `.svelte` imports
  fail the build.
- This project uses neither `import.meta.env` nor Vite's asset-query imports
  today, so the second reference is currently precautionary — it is part of the
  standard Vite template and costs nothing.

## See also

[[tsconfig.json]] · [[vite.config.ts]] · [[package.json]] ·
[[frontend-svelte]] · [[MOC]]
