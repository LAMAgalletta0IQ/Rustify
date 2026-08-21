---
tags: [file, config, build, frontend]
---
# `tsconfig.json`

**Module:** [[project-root]] · **Language:** JSON

## Purpose

TypeScript configuration for the frontend and for `svelte-check`.

## Key items

### Module and target
`target: "ES2022"`, `module: "ESNext"`, `moduleResolution: "bundler"`,
`isolatedModules: true`, `moduleDetection: "force"`,
`verbatimModuleSyntax: true`.

`"bundler"` resolution matches Vite's behaviour (extensionless imports, package
`exports` maps). `verbatimModuleSyntax` requires `import type` for type-only
imports — visible throughout [[api.ts]] and the components.

### Strictness
```json
"strict": true,
"noUnusedLocals": true,
"noUnusedParameters": true,
"noFallthroughCasesInSwitch": true
```

> **`noUnusedLocals` and `noUnusedParameters` are build-breaking**, because
> [[package.json]]'s `build` script runs `svelte-check` first. A leftover
> import fails the release build, not just the editor.

### Libraries
`lib: ["ES2022", "DOM", "DOM.Iterable"]`, `skipLibCheck: true`,
`allowJs` and `checkJs` both on — so [[svelte.config.js]] is type-checked too.

### Include
```json
"include": ["src/**/*.ts", "src/**/*.svelte", "vite.config.ts"]
```
Picks up [[vite-env.d.ts]] via the glob. Note `src-tauri/` is absent — that is
Rust.

## Inputs / outputs / side effects

Read by `svelte-check` and by editors. **Emits nothing** — there is no `outDir`
and no `tsc` build step; Vite/esbuild handles transpilation and TypeScript is
used purely for checking.

## Dependencies

**Referenced by:** [[package.json]] scripts (`--tsconfig ./tsconfig.json`)
**Covers:** all of `src/`, plus [[vite.config.ts]]

## Notable logic / gotchas

- **Type checking is the only guard on the IPC contract's frontend half.**
  [[types.ts]] mirrors the Rust structs by hand, and nothing verifies the two
  agree — strictness here at least catches misuse *within* TypeScript. See
  [[architecture]].
- **`strict: true` makes `null` explicit**, which is why [[types.ts]] uses
  `string | null` for Rust `Option` fields rather than optional properties.
- **`isolatedModules`** is required because esbuild transpiles file by file
  with no cross-file type information.
- No path aliases; all imports are relative.

## See also

[[types.ts]] · [[vite-env.d.ts]] · [[package.json]] · [[svelte.config.js]] ·
[[architecture]] · [[project-root]] · [[MOC]]
