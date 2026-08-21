---
tags: [file, backend, entrypoint, rust]
---
# `src-tauri/src/lib.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **~203 lines**

> Until 2026-08 this counted 111 lines, 11 modules, and 43 commands. Every
> module added since then (see [[backend-rust]] for the full list) is
> declared here, and `generate_handler!` now lists 82 commands.

## Purpose

The crate root and application assembly point. Loads `.env`, declares every
backend module, configures the Tauri builder, and registers all 82 commands.
The single best file to read first — it is the table of contents for the
backend.

## Key items

### Module declarations
```rust
mod audio; mod audio_capabilities; mod auth; mod commands; mod connect;
mod error; mod friends; pub mod jams; mod jams_bridge; mod library;
mod lyrics; mod media_keys; mod music_videos; mod player; mod podcasts;
mod profiles; mod queue; mod relevance; mod remote_state; mod search;
mod sleep_timer; mod spotify; mod state; mod telemetry; mod webapi;
```
All private except `jams`, which is `pub` because
`examples/jam_demo.rs` drives it directly from outside the crate as a
standalone demo — not because it is any less wired into the app than
anything else here. It reaches the UI the same way every other feature
module does: through [[jams_bridge.rs]] and the jam commands in
[[commands.rs]].

> Until 2026-08 the source comment above `pub mod jams;` claimed the module
> was "not yet exposed to the UI" and existed only so `examples/jam_demo.rs`
> could compile against it. That was already false by the time this was
> reviewed — [[Jams.svelte]] is a real, reachable view (from the player
> bar's Jam button) and all nine jam commands are registered — so the
> comment was corrected in source rather than left to mislead the next
> reader into thinking Jam is dead code.

### `fn load_dotenv() -> Option<PathBuf>`

Loads `.env` into the process environment. `dotenvy::dotenv()` walks up from the
working directory — `src-tauri/` under `tauri dev`, so a project-root `.env` is
found. Falls back to the executable's own directory for bundled builds, whose
launch directory is arbitrary.

Returns the path rather than logging it: this runs *before* the logger exists.

### `pub fn run()`

Annotated `#[cfg_attr(mobile, tauri::mobile_entry_point)]` — inert on desktop,
kept from the Tauri template. Executes in order:

1. **`load_dotenv()`** — must precede the logger so `RUST_LOG` can live in the
   file.
2. **Logging** — `env_logger` with `default_filter_or("info,librespot=warn")`.
   librespot is extremely verbose at `debug`; `warn` keeps its noise out while
   leaving app logs at `info`. `RUST_LOG` overrides both.
3. **Startup report** — logs the `.env` path (or its absence) and whether the
   Web API client ID is private or the shared default. The fastest way to tell
   which quota is in play; see [[rate-limiting]].
4. **`tauri_plugin_opener`** — used by librespot's OAuth helper to open the
   system browser.
5. **`tauri_plugin_global_shortcut`** — backs [[media_keys.rs]].
6. **`.setup(...)`** — calls `media_keys::register(app.handle())`.
7. **`.manage(AppState::new())`** — installs shared state ([[state.rs]]).
8. **`.invoke_handler(generate_handler![...])`** — the 82 commands.

Debug builds also register `tauri-plugin-mcp-bridge` on `127.0.0.1:9223` for
Tauri MCP inspection. `cfg(debug_assertions)` removes it from the verified
release/NSIS build; loopback binding prevents LAN exposure during development.
9. **`.run(generate_context!())`** — reads [[tauri.conf.json]] at compile time;
   blocks until exit.

## The command registry

Grouped by comment in the source — see [[commands.rs]] for the full,
up-to-date per-command breakdown, which this table only summarizes:

| Group | Commands |
| --- | --- |
| auth | `get_auth_state`, `get_login_info`, `set_client_id`, `login`, `start_device_authorization`, `complete_device_authorization`, `cancel_device_authorization`, `restore_session`, `logout` |
| settings / audio config | `get_settings`, `update_settings`, `configure_audio`, `update_audio_settings`, `list_audio_devices`, `get_audio_status`, `get_equalizer_presets` |
| playback | `get_playback`, `play`, `pause`, `play_pause`, `next_track`, `previous_track`, `seek`, `set_volume`, `set_shuffle`, `set_repeat`, `load_context`, `load_tracks` |
| sleep timer | `get_sleep_timer`, `start_sleep_timer`, `sleep_at_end_of_track`, `cancel_sleep_timer` |
| connect | `list_devices`, `transfer_playback`, `activate_this_device` |
| library / artist | collection reads and saves, `get_followed_releases`, `get_quick_access`, `record_relevance`, `get_artist_overview`, `get_track_credits`, user top tracks/artists |
| DJ / Home | `get_personalized_home`, `get_dj_status`, `start_dj` |
| episode / music video / audio / telemetry | `get_episode_resume`, `set_episode_completed`, `get_music_video_capability`, `get_audio_capability`, `get_telemetry_status` |
| social | `get_friend_activity`, `get_user_profile`, `search_users` |
| lyrics | `get_lyrics` |
| search / queue | `search_spotify`, `get_queue`, `add_to_queue` |
| jams | `get_jam_status`, `create_jam`, `refresh_jam`, `join_jam`, `leave_jam`, `add_track_to_jam`, `set_jam_queue_control`, `kick_jam_member`, `end_jam` |

## Inputs / outputs / side effects

- **Reads** [[tauri.conf.json]] at compile time via `generate_context!()`.
- **Side effects:** initialises the logger, registers global media keys,
  creates the window, blocks the main thread.

## Dependencies

**Imports:** every backend module; `tauri`, `env_logger`,
`tauri_plugin_opener`, `tauri_plugin_global_shortcut`
**Imported by:** [[main.rs]]

## Notable logic / gotchas

- **`.env` must load before the logger.** `RUST_LOG` is read at
  `env_logger::init()`, so a `.env` loaded afterwards would silently not apply.
  The consequence is that `load_dotenv` cannot log its own outcome — an earlier
  version called `log::debug!` inside it, which compiled, ran, and emitted
  nothing at all because no logger was installed yet.
- **Ordering matters.** `.setup()` runs before any login, so
  [[media_keys.rs]] handlers must tolerate `spotify == None`. They do —
  each returns early when logged out.
- **A command not listed here does not exist to the frontend.** Adding
  `#[tauri::command]` in [[commands.rs]] without adding it to
  `generate_handler!` produces a runtime "command not found", never a compile
  error. This is the most common way to break the IPC contract.
- **`generate_handler!` is a macro over identifiers**, so the list cannot be
  built programmatically — it must be maintained by hand alongside [[api.ts]].
- Every command name here must match a string in [[api.ts]] exactly.

## See also

[[main.rs]] · [[commands.rs]] · [[state.rs]] · [[media_keys.rs]] ·
[[api.ts]] · [[entry-points]] · [[architecture]] · [[backend-rust]] · [[MOC]]
