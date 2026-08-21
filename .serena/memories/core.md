# Rustify core
- Windows-only Tauri 2 Spotify client. Rust owns HTTP, OAuth, audio, persistence, error mapping, and external integrations; Svelte 5 owns presentation and invokes typed IPC only.
- IPC changes must stay synchronized across `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/api.ts`, and `src/lib/types.ts`.
- Playback UI mirrors authoritative Rust `PlaybackState` via `playback:changed`. A webview reload does not restart Rust; startup must reuse a live backend session before attempting refresh-token restore.
- App data separates `tokens.json` from durable `settings.json` (client ID, default volume, reduced motion, cache limit).
- Spotify/librespot capability facts are in `mem:backend/spotify_2026`; frontend structure in `mem:frontend/core`; backend structure in `mem:backend/core`.
- The Obsidian vault is the architecture record. Material behavior, external limits, endpoints, and verification results must be updated there.