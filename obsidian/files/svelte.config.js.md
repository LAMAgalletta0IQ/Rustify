---
tags: [file, config, build, frontend]
---
# `svelte.config.js`

**Module:** [[project-root]] · **Language:** JavaScript (ESM)

## Purpose

Svelte compiler configuration. Minimal — it exists solely to enable
TypeScript inside `.svelte` files.

## Contents

```js
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  preprocess: vitePreprocess(),
};
```

## Key items

### `vitePreprocess()`
Routes `<script lang="ts">` blocks through Vite's esbuild transform before the
Svelte compiler sees them. **Every component in this project uses
`lang="ts"`**, so without this the build fails immediately.

It also handles `<style>` preprocessing, though no CSS preprocessor is used
here — styles are plain scoped CSS ([[app.css]] holds the shared tokens).

### ESM syntax
`export default` rather than `module.exports`, because [[package.json]] sets
`"type": "module"`.

## Inputs / outputs / side effects

Read by `@sveltejs/vite-plugin-svelte`, which is registered in
[[vite.config.ts]]. No direct output.

## Dependencies

**Imports:** `@sveltejs/vite-plugin-svelte`
**Read by:** the Svelte Vite plugin, and by `svelte-check`

## Notable logic / gotchas

- **No adapter, no SvelteKit.** This is plain Svelte 5 with Vite — there is no
  routing, SSR, or file-based page system. Navigation is component state; see
  [[frontend-views]].
- **`svelte-check` reads this too**, so a preprocessor change affects type
  checking as well as bundling.
- `compilerOptions` is absent, so runes mode is inferred per file from usage —
  the Svelte 5 default.

## See also

[[vite.config.ts]] · [[package.json]] · [[tsconfig.json]] ·
[[frontend-svelte]] · [[project-root]] · [[MOC]]
