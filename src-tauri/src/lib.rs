mod audio;
mod audio_capabilities;
mod auth;
mod commands;
mod connect;
mod error;
mod friends;
// Experimental Spotify Jams module. Self-contained; see README_jams.md. Not yet
// exposed to the UI — the app only links it so it compiles and can be driven
// from examples/jam_demo.rs. Remove this line if you want to keep it out of the
// build for now.
pub mod jams;
mod jams_bridge;
mod library;
mod lyrics;
mod media_keys;
mod music_videos;
mod player;
mod podcasts;
mod profiles;
mod queue;
mod relevance;
mod remote_state;
mod search;
mod sleep_timer;
mod spotify;
mod state;
mod telemetry;
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
        if std::env::var(auth::CLIENT_ID_ENV).is_ok_and(|s| !s.trim().is_empty()) {
            "private (from environment)"
        } else {
            "not set via environment — reading settings.json, or prompting first-run Setup if absent"
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
            commands::set_client_id,
            commands::get_settings,
            commands::update_settings,
            commands::list_audio_devices,
            commands::get_audio_status,
            commands::get_equalizer_presets,
            commands::login,
            commands::start_device_authorization,
            commands::complete_device_authorization,
            commands::cancel_device_authorization,
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
            commands::get_sleep_timer,
            commands::start_sleep_timer,
            commands::sleep_at_end_of_track,
            commands::cancel_sleep_timer,
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
            commands::get_followed_artists,
            commands::get_followed_releases,
            commands::get_recently_played,
            commands::get_quick_access,
            commands::record_relevance,
            commands::set_tracks_saved,
            commands::set_albums_saved,
            commands::set_artists_saved,
            commands::get_tracks_saved,
            commands::get_albums_saved,
            commands::get_artists_saved,
            commands::get_liked_tracks_by_artist,
            commands::get_artist_top_tracks,
            commands::get_artist_albums,
            commands::get_artist,
            commands::get_artist_concerts,
            commands::get_track_credits,
            commands::get_episode_resume,
            commands::set_episode_completed,
            commands::get_telemetry_status,
            commands::get_music_video_capability,
            commands::get_audio_capability,
            commands::get_top_tracks,
            commands::get_top_artists,
            commands::get_personalized_home,
            commands::get_dj_status,
            commands::start_dj,
            commands::get_lyrics,
            commands::get_friend_activity,
            commands::get_user_profile,
            commands::search_users,
            // search
            commands::search_spotify,
            // queue
            commands::get_queue,
            commands::add_to_queue,
            // jams (experimental)
            commands::get_jam_status,
            commands::create_jam,
            commands::refresh_jam,
            commands::join_jam,
            commands::leave_jam,
            commands::add_track_to_jam,
            commands::set_jam_queue_control,
            commands::kick_jam_member,
            commands::end_jam,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
