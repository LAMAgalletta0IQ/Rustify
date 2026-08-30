//! Tauri command layer, split by domain.
//!
//! Every `#[tauri::command]` function used to live in one 2,200+ line
//! `commands.rs`. It is now split into `session` (auth/settings/audio
//! config/login lifecycle), `playback` (transport/Connect/queue),
//! `library` (playlists/saved items/search), `discovery`
//! (DJ/Home/DNA/lyrics/credits/profiles and other mostly-private-API extras)
//! and `jams`. This module re-exports every command from each of those, so
//! `lib.rs`'s `generate_handler![commands::get_auth_state, ...]` list needed
//! no changes — every entry still resolves the same way it did when they were
//! all declared directly in `commands.rs`.
//!
//! The handful of helpers below (`token`, `with_spirc`, `remote_put`/
//! `remote_post`, the crossfade-transition helpers, `device_name`) are used
//! across more than one domain, so they stay here rather than picking an
//! arbitrary owning module; submodules pull them in with `use super::*`.

use tauri::{AppHandle, Manager};

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

mod discovery;
mod jams;
mod library;
mod playback;
mod session;

pub use discovery::*;
pub use jams::*;
pub use library::*;
pub use playback::*;
pub use session::*;

/// Pulls the Web API bearer token out of the live session, or fails cleanly if
/// the user is not logged in. Every Web API command starts here.
pub(crate) async fn token(state: &AppState) -> AppResult<String> {
    if state.spotify.read().await.is_none() {
        return Err(AppError::NotLoggedIn);
    }
    let token = state.tokens.get().await;
    if token.is_empty() {
        return Err(AppError::SessionExpired);
    }
    Ok(token)
}

/// Runs `f` against the live Spirc handle.
pub(crate) async fn with_spirc<F>(state: &AppState, f: F) -> AppResult<()>
where
    F: FnOnce(&librespot::connect::Spirc) -> Result<(), librespot::core::Error>,
{
    let guard = state.spotify.read().await;
    let s = guard.as_ref().ok_or(AppError::NotLoggedIn)?;
    f(&s.spirc).map_err(AppError::from)
}

pub(crate) fn configured_crossfade(app: &AppHandle) -> bool {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| auth::settings_or_default(&dir).crossfade_seconds > 0)
        .unwrap_or(false)
}

pub(crate) fn should_clear_crossfade(enabled: bool, local_active: bool, playing: bool) -> bool {
    enabled && local_active && playing
}

/// Pause is the public seam that makes the pinned librespot crossfade PR drop
/// its outgoing decoder. Return true when callers must restore playback after
/// their transition command.
pub(crate) async fn clear_crossfade_before_transition(
    app: &AppHandle,
    state: &AppState,
) -> AppResult<bool> {
    let (local_active, playing) = {
        let playback = state.playback.read().await;
        (playback.is_active_device, playback.is_playing)
    };
    let clear = should_clear_crossfade(configured_crossfade(app), local_active, playing);
    if clear {
        with_spirc(state, |spirc| spirc.pause()).await?;
    }
    Ok(clear)
}

pub(crate) async fn remote_put(
    state: &AppState,
    path: &str,
    query: &[(&str, String)],
) -> AppResult<()> {
    state
        .web_api
        .put_query(&token(state).await?, path, query)
        .await
}

pub(crate) async fn remote_post(
    state: &AppState,
    path: &str,
    query: &[(&str, String)],
) -> AppResult<()> {
    state
        .web_api
        .post_query(&token(state).await?, path, query)
        .await
}

pub(crate) fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| format!("{s} (Rustify)"))
        .unwrap_or_else(|| "Rustify".to_string())
}

#[cfg(test)]
mod settings_validation_tests {
    use super::*;

    #[test]
    fn should_clear_crossfade_requires_enabled_active_and_playing() {
        assert!(should_clear_crossfade(true, true, true));
        assert!(!should_clear_crossfade(false, true, true));
        assert!(!should_clear_crossfade(true, false, true));
        assert!(!should_clear_crossfade(true, true, false));
    }
}

/// Registration-parity checks between the Rust command surface and the
/// frontend. Crude and text-based, per CLAUDE.md's documented three-edit
/// registration hazard (`commands.rs`, `lib.rs`'s `generate_handler![]`, and
/// `src/lib/api.ts` must all agree, and missing edit 2 or 3 previously failed
/// only at *runtime* with "command not found"). This will not catch a
/// command name built from a variable rather than a string literal — neither
/// side does that today — and it does not check argument shapes, only that
/// both sides agree on which command/event names exist.
#[cfg(test)]
mod registration_parity_tests {
    use std::collections::HashSet;

    const LIB_RS: &str = include_str!("../lib.rs");
    const STATE_RS: &str = include_str!("../state.rs");
    const API_TS: &str = include_str!("../../../src/lib/api.ts");

    /// Every `commands::foo` entry inside `tauri::generate_handler![...]`.
    fn registered_commands() -> Vec<String> {
        let start = LIB_RS
            .find("generate_handler![")
            .expect("generate_handler! block not found in lib.rs");
        let rest = &LIB_RS[start..];
        let end = rest
            .find(']')
            .expect("generate_handler! block not closed with ']'");
        rest[..end]
            .lines()
            .filter_map(|line| line.trim().trim_end_matches(',').strip_prefix("commands::"))
            .map(str::to_string)
            .collect()
    }

    /// Every command name string literal passed to `invoke(...)`/`invoke<T>(...)`
    /// in `api.ts`.
    fn invoked_commands() -> HashSet<String> {
        let mut commands = HashSet::new();
        let mut rest = API_TS;
        while let Some(pos) = rest.find("invoke") {
            rest = &rest[pos + "invoke".len()..];
            let after = rest.trim_start();
            // Skip an optional `<Type>` turbofish before the argument list.
            let after = match after.strip_prefix('<') {
                Some(stripped) => match stripped.find('>') {
                    Some(gt) => &stripped[gt + 1..],
                    None => continue,
                },
                None => after,
            };
            let Some(paren) = after.find('(') else {
                continue;
            };
            let after_paren = after[paren + 1..].trim_start();
            let Some(quoted) = after_paren.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = quoted.find('"') {
                commands.insert(quoted[..end].to_string());
            }
        }
        commands
    }

    #[test]
    fn every_registered_command_has_a_frontend_wrapper() {
        let registered = registered_commands();
        assert!(
            registered.len() > 50,
            "sanity check: parsed too few commands out of generate_handler! ({} found) \
             - the text-based parser likely broke on a formatting change",
            registered.len()
        );
        let invoked = invoked_commands();
        let missing: Vec<_> = registered
            .iter()
            .filter(|name| !invoked.contains(*name))
            .collect();
        assert!(
            missing.is_empty(),
            "commands registered in lib.rs's generate_handler! but never invoked from \
             src/lib/api.ts (dead on the frontend, or api.ts is missing a wrapper): {missing:?}"
        );
    }

    #[test]
    fn every_frontend_invoke_call_names_a_registered_command() {
        let registered: HashSet<_> = registered_commands().into_iter().collect();
        let invoked = invoked_commands();
        let unknown: Vec<_> = invoked
            .iter()
            .filter(|name| !registered.contains(*name))
            .collect();
        assert!(
            unknown.is_empty(),
            "src/lib/api.ts invokes commands that are not registered in lib.rs's \
             generate_handler! (these fail at runtime with \"command not found\"): {unknown:?}"
        );
    }

    /// Extracts every `"..."` string literal within `text`. No escape
    /// handling — sufficient for the plain identifier-like strings both
    /// `state::events` and api.ts's `EVENT_*` constants hold.
    fn quoted_strings(text: &str) -> HashSet<String> {
        let mut values = HashSet::new();
        let mut rest = text;
        while let Some(start) = rest.find('"') {
            rest = &rest[start + 1..];
            let Some(end) = rest.find('"') else { break };
            values.insert(rest[..end].to_string());
            rest = &rest[end + 1..];
        }
        values
    }

    /// The wire values inside `state::events` (e.g. `"playback:changed"`),
    /// not the Rust constant names — those are compared against api.ts's
    /// `EVENT_*` values, whose own identifier names differ (`EVENT_PLAYBACK`
    /// vs `PLAYBACK`) by convention on each side.
    fn rust_event_values() -> HashSet<String> {
        let start = STATE_RS
            .find("pub mod events")
            .expect("events module not found in state.rs");
        let rest = &STATE_RS[start..];
        let open = rest.find('{').expect("events module has no body");
        let mut depth = 0i32;
        let mut end = open;
        for (i, ch) in rest[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        quoted_strings(&rest[open..=end])
    }

    fn ts_event_values() -> HashSet<String> {
        API_TS
            .lines()
            .filter(|line| line.trim_start().starts_with("export const EVENT_"))
            .flat_map(|line| quoted_strings(line))
            .collect()
    }

    #[test]
    fn event_wire_values_match_between_rust_and_frontend() {
        let rust = rust_event_values();
        let ts = ts_event_values();
        assert!(
            !rust.is_empty(),
            "sanity check: found no state::events values"
        );
        assert!(
            !ts.is_empty(),
            "sanity check: found no api.ts EVENT_* values"
        );
        assert_eq!(
            rust, ts,
            "state::events's wire values and src/lib/api.ts's EVENT_* values must match exactly"
        );
    }
}
