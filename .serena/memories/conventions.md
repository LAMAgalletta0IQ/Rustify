# Conventions
- Frontend: Svelte 5 runes, callback props, strict TypeScript, relative imports, scoped plain CSS consuming tokens from `src/app.css`.
- Components never call Tauri `invoke` directly; all IPC goes through `src/lib/api.ts`.
- Rust feature modules flatten private Spotify `Wire*` payloads into camelCase IPC summary structs; commands delegate and contain little domain logic.
- Use `AppResult<T>` and structured `AppError` kinds across IPC. Do not expose access/refresh tokens or raw secrets in messages/logs.
- Tokio RwLocks are used for async shared state. Explicitly abort background JoinHandles; dropping detaches tasks.
- Playback actions go through Spirc for this device; server-side Web API owns metadata, library mutation, queue, and outbound device transfer.
- Effects that call async loaders use cancellation guards; guard tracked synchronous reads with `untrack` where necessary.
- Preserve the warm Acrylic visual system and low-footprint design; prefer targeted additions over framework adoption.