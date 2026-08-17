---
tags: [file, backend, entrypoint, rust]
---
# `src-tauri/src/lib.rs`

**Module:** [[backend-rust]] · **Language:** Rust · **111 lines**

## Purpose

The crate root and application assembly point. Loads `.env`, declares every
backend module, configures the Tauri builder, and registers all 33 commands.
The single best file to read first — it is the table of contents for the
backend.

## Key items

### Module declarations
```rust
mod auth; mod commands; mod connect; mod error; mod library;
mod media_keys; mod player; mod queue; mod search; mod state; mod webapi;
```
All private: nothing in this crate is a public API surface except `run()`.

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
8. **`.invoke_handler(generate_handler![...])`** — the 33 commands.
9. **`.run(generate_context!())`** — reads [[tauri.conf.json]] at compile time;
   blocks until exit.

## The command registry

Grouped by comment in the source:

| Group | Commands |
| --- | --- |
| auth | `get_auth_state`, `get_login_info`, `login`, `restore_session`, `logout` |
| playback | `get_playback`, `play`, `pause`, `play_pause`, `next_track`, `previous_track`, `seek`, `set_volume`, `set_shuffle`, `set_repeat`, `load_context`, `load_tracks` |
| connect | `list_devices`, `transfer_playback`, `activate_this_device` |
| library | `get_playlists`, `get_playlist_tracks`, `get_saved_tracks`, `get_saved_albums`, `get_album_tracks`, `set_tracks_saved`, `set_albums_saved`, `get_tracks_saved`, `get_artist_top_tracks`, `get_artist_albums` |
| search | `search_spotify` |
| queue | `get_queue`, `add_to_queue` |

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
