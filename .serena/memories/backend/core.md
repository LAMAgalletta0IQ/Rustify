# Backend core
- `state.rs`: AppState/AuthState/PlaybackState/TokenStore/live SpotifySession. `commands.rs` is the sole Tauri IPC surface; `lib.rs` registers 43 commands.
- Two OAuth roles: librespot desktop client for streaming/Spirc; user-configured client for Spotify Web API. Refresh tokens persist separately; active webview reloads reuse the live session.
- Background refresh retries transient failures but terminal invalid_grant/invalid_client clears rejected credentials, shuts the session, resets auth/playback, emits auth logged-out, and stops. Refresh tokens expire after six months as of 2026.
- `Settings` persists client ID + default_volume_percent=50 + reduce_motion=false + cache_limit_mb=2048 with legacy serde defaults. Updates merge the client ID. Player uses volume/cache values on next session.
- `webapi.rs` maps 400/401/403/404/429/5xx to typed errors, accepts empty 200/204, retries short GET 429 only, and offers query PUT/DELETE with explicit Content-Length: 0.
- `library.rs` owns current Spotify contracts and context-aware recent activity. `lyrics.rs` is a separate documented LRCLIB client/parser.
- Debug-only Tauri MCP bridge binds 127.0.0.1:9223 under cfg(debug_assertions); it is absent from release builds.