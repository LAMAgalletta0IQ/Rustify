---
tags: [file, config, build]
---
# `package.json`

**Module:** [[project-root]] · **Language:** JSON

## Purpose

npm manifest: scripts, frontend dependencies, and the install-script approval
the build depends on.

## Key items

### Identity
`name: "spotify-rust"`, `private: true`, `version: "0.1.0"`,
**`type: "module"`** — so `.js` files are ES modules, which is why
[[svelte.config.js]] uses `export default`.

### Scripts
| Script | Command | Notes |
| --- | --- | --- |
| `dev` | `vite` | Invoked by Tauri's `beforeDevCommand` |
| `build` | `svelte-check --tsconfig ./tsconfig.json && vite build` | **Type errors fail the build** |
| `preview` | `vite preview` | Unused in the Tauri flow |
| `check` | `svelte-check --tsconfig ./tsconfig.json` | Type check alone |
| `tauri` | `tauri` | Backs `npm run tauri dev/build/icon` |

### Dependencies
`@tauri-apps/api` (`invoke`, `listen`), `@tauri-apps/plugin-opener`.

### devDependencies
`@sveltejs/vite-plugin-svelte` 5, `@tauri-apps/cli` 2, `svelte` 5,
`svelte-check` 4, `tslib` 2, `typescript` 5, `vite` 6.

### `allowScripts`
```json
"allowScripts": { "esbuild@0.25.12": true }
```

> **This block is required for the build to work.** npm blocks package install
> scripts by default in this environment. `esbuild` needs its `postinstall` to
> fetch the platform binary; without it, Vite cannot bundle and
> `vite build` fails. Recorded here and in [[package-lock.json]].
>
> After a fresh clone, if `vite build` fails on esbuild:
> ```powershell
> npm approve-scripts esbuild
> npm rebuild esbuild
> ```

## Inputs / outputs / side effects

Read by npm and by the Tauri CLI (via `beforeDevCommand` /
`beforeBuildCommand` in [[tauri.conf.json]]).

## Dependencies

**Pairs with:** [[package-lock.json]]
**Referenced by:** [[tauri.conf.json]]

## Notable logic / gotchas

- **`build` runs `svelte-check` first**, so a type error in any `.svelte` or
  `.ts` file blocks a release build. Deliberate — the IPC contract in
  [[types.ts]] is only checked on the TypeScript side.
- **`npm run tauri` passes through to the CLI**; extra flags need a `--`
  separator, e.g. `npm run tauri build -- --no-bundle`.
- Version ranges are caret-loose (`"^5"`); exact versions live in the lockfile.
- `tslib` is a transitive requirement of TypeScript's helper emit, not used
  directly.

## See also

[[package-lock.json]] · [[vite.config.ts]] · [[svelte.config.js]] ·
[[tsconfig.json]] · [[tauri.conf.json]] · [[build-and-config]] ·
[[project-root]] · [[MOC]]
