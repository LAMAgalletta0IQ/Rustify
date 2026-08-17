---
tags: [concept, entrypoint]
---
# Entry points

Every way execution begins, in boot order.

## 1. Process start — `main()`

[[main.rs]] is the OS entry point and does exactly two things: suppress the
console window on release builds (`windows_subsystem = "windows"`), then call
`rustify_lib::run()`.

The real work is in [[lib.rs]] because [[Cargo.toml]] declares both a binary
and a library target — the library form is what a future mobile entry point
would call.

## 2. Application setup — `run()`

[[lib.rs]] `run()` executes in this order:

1. **Logging** — `env_logger` at `info`, with `librespot=warn` (librespot is
   extremely chatty at `debug`). Override with `RUST_LOG`.
2. **Plugins** — `tauri-plugin-opener` (opens the browser for OAuth) and
   `tauri-plugin-global-shortcut` (media keys).
3. **`setup` hook** — calls `media_keys::register()` ([[media_keys.rs]]).
   This happens *before* any login, so the handlers must tolerate a
   logged-out state.
4. **`manage(AppState::new())`** — installs shared state ([[state.rs]]).
   Everything is empty at this point.
5. **`invoke_handler`** — registers all 33 commands from [[commands.rs]].
6. **`run(generate_context!())`** — reads [[tauri.conf.json]] at compile time,
   creates the window, and blocks.

## 3. Webview start — the frontend

The window loads the frontend, which boots independently:

- [[index.html]] is the HTML entry; Vite rewrites its `<script>` tag.
- [[main.ts]] calls Svelte 5's `mount()` on [[App.svelte]] into `#app`, and
  imports [[app.css]].

Where the HTML comes from differs by mode:

| Mode | Source | Set in |
| --- | --- | --- |
| `npm run tauri dev` | Vite dev server on `http://localhost:1420` | `devUrl` in [[tauri.conf.json]] |
| `npm run tauri build` | `../dist`, compiled *into* the binary | `frontendDist` |

That second row matters: in a release build the frontend is baked into the
`.exe` at compile time. Rebuilding `dist/` afterwards changes nothing until
cargo recompiles. See [[build-and-config]].

## 4. First render — session restore

[[App.svelte]] calls `store.init()` on mount ([[store.svelte.ts]]), which:

1. Subscribes to the `playback:changed` and `auth:changed` Tauri events.
2. Calls `restore_session` ([[commands.rs]]).
3. Sets `booting = false` in a `finally`, so a thrown error still reaches the
   login screen rather than hanging on "Starting…".

`restore_session` reads the stored refresh token ([[auth.rs]]). Three outcomes:

- **No token file** → returns a default logged-out `AuthState`; [[Login.svelte]]
  renders. Not an error.
- **Token present and valid** → refreshes, verifies Premium, starts librespot,
  and the main UI renders. No browser needed.
- **Token rejected** → the stored token is deleted and a logged-out state is
  returned, so a revoked session degrades to the login screen rather than an
  error dialog.

## Runtime entry points (after boot)

Execution also enters from three places that are *not* user clicks — each runs
on its own task and can mutate state at any time:

| Trigger | Handler | Notes |
| --- | --- | --- |
| **Media keys** | [[media_keys.rs]] | Fires even when unfocused; no-ops when logged out |
| **`PlayerEvent` stream** | `spawn_event_pump` in [[player.rs]] | librespot pushes; drives all UI updates |
| **Remote Connect control** | librespot `Spirc` task | A phone can pause playback with no local input |
| **Token refresher** | `spawn_refresher` in [[auth.rs]] | Wakes ~5 min before expiry |

The third is the one that surprises people: the UI must react to state changes
it did not initiate. That is why the frontend renders from pushed events rather
than from command return values. See [[state-and-events]].

## See also

[[architecture]] · [[data-flow]] · [[state-and-events]] · [[main.rs]] ·
[[lib.rs]] · [[App.svelte]] · [[store.svelte.ts]] · [[MOC]]
