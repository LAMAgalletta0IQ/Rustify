---
tags: [module, config]
---
# Module — Project root

**Path:** repository root

Frontend tooling config and project-level files. The Rust half lives under
[[tauri-config]].

## Files

| File | Role |
| --- | --- |
| [[package.json]] | npm scripts, deps, `allowScripts` approval |
| [[package-lock.json]] | Exact npm versions |
| [[vite.config.ts]] | Dev server port, build target, watch exclusions |
| [[svelte.config.js]] | Svelte preprocessor |
| [[tsconfig.json]] | TypeScript strictness |
| [[index.html]] | HTML entry for Vite |
| [[.gitignore]] | Ignore rules |
| [[.gitattributes]] | LF normalisation |
| [[README.md]] | Human-facing docs, setup, test checklist, measurements |
| [[icon.svg]] | Source image for the generated [[icons]] |
| `.env.example` | Committed template for local config — see [[build-and-config]] |
| `.env` | Local config, gitignored. `RUSTIFY_CLIENT_ID`, optionally `RUST_LOG` |

## Directory layout

```
rustify/
├── index.html          ← Vite entry
├── package.json        ← npm scripts
├── vite.config.ts
├── svelte.config.js
├── tsconfig.json
├── icon.svg             ← icon source
├── README.md
├── .env.example        ← config template (committed)
├── .env                ← local config (gitignored)
├── src/                → [[frontend-svelte]]
├── src-tauri/          → [[backend-rust]] + [[tauri-config]]
├── obsidian/           ← this vault
├── node_modules/       ← excluded
└── dist/               ← excluded (build output)
```

Standard Tauri layout: frontend at the root, Rust in `src-tauri/`. Vite treats
the root as its project root, which is why [[index.html]] sits beside
[[package.json]] rather than under `src/`.

## Scripts

| Script | Runs |
| --- | --- |
| `dev` | `vite` |
| `build` | `svelte-check` then `vite build` — type errors fail the build |
| `check` | `svelte-check` alone |
| `tauri` | The Tauri CLI (`tauri dev`, `tauri build`, `tauri icon`) |

## See also

[[build-and-config]] · [[frontend-svelte]] · [[tauri-config]] ·
[[external-dependencies]] · [[MOC]]
