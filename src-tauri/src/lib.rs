mod auth;
mod commands;
mod connect;
mod error;
mod library;
mod media_keys;
mod player;
mod queue;
mod search;
mod state;
mod webapi;

use std::path::{Path, PathBuf};

use state::AppState;

/// Loads `.env` into the process environment.
///
/// Must run before anything reads a variable — including the logger, so
/// `RUST_LOG` can live in the file too.
///
/// Real environment variables always win: `dotenvy` does not overwrite what is
/// already set, so `$env:RUSTIFY_CLIENT_ID = "..."` still overrides the
/// file for a one-off run.
/// Returns the file it loaded, if any. Cannot log on its own: it runs before
/// the logger exists, so anything it emitted would be swallowed. The caller
/// reports the result once logging is up.
fn load_dotenv() -> Option<PathBuf> {
    // Walks up from the working directory, which under `tauri dev` is
    // `src-tauri/` — so a `.env` at the project root is found.
    if let Ok(path) = dotenvy::dotenv() {
        return Some(path);
    }

    // A bundled app is launched from an arbitrary directory, so fall back to
    // the one holding the executable.
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))?;
    let path = dir.join(".env");
    dotenvy::from_path(&path).ok().map(|_| path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before the logger, so `RUST_LOG` can live in the file too.
    let dotenv_path = load_dotenv();

    // librespot is chatty at debug; keep it at info unless RUST_LOG overrides.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,librespot=warn"),
    )
    .init();

    match &dotenv_path {
        Some(p) => log::info!("loaded environment from {}", p.display()),
        None => log::debug!("no .env found; using the ambient environment only"),
    }
    log::info!(
        "web api client id: {}",
        if auth::webapi_client_id().is_some() {
            "private (from environment)"
        } else {
            "shared librespot default — see .env.example"
        }
    );

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    // Automation bridge for tauri-mcp, so the UI can be driven and screenshotted
    // from outside. Debug-only — it never reaches a release build. Bound to
    // loopback rather than the default 0.0.0.0: it is an unauthenticated
    // control channel and has no business being reachable from the network.
    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(
            tauri_plugin_mcp_bridge::Builder::new()
                .bind_address("127.0.0.1")
                .build(),
        );
    }

    builder
        .setup(|app| {
            media_keys::register(app.handle());
            Ok(())
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // auth
            commands::get_auth_state,
            commands::get_login_info,
            commands::login,
            commands::restore_session,
            commands::logout,
            // playback
            commands::get_playback,
            commands::play,
            commands::pause,
            commands::play_pause,
            commands::next_track,
            commands::previous_track,
            commands::seek,
            commands::set_volume,
            commands::set_shuffle,
            commands::set_repeat,
            commands::load_context,
            commands::load_tracks,
            // connect
            commands::list_devices,
            commands::transfer_playback,
            commands::activate_this_device,
            // library
            commands::get_playlists,
            commands::get_playlist_tracks,
            commands::get_saved_tracks,
            commands::get_saved_albums,
            commands::get_album_tracks,
            commands::probe_webapi,
            commands::get_followed_artists,
            commands::get_recently_played,
            commands::set_tracks_saved,
            commands::set_albums_saved,
            commands::get_tracks_saved,
            commands::get_artist_top_tracks,
            commands::get_artist_albums,
            // search
            commands::search_spotify,
            // queue
            commands::get_queue,
            commands::add_to_queue,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
