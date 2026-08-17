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

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // librespot is chatty at debug; keep it at info unless RUST_LOG overrides.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,librespot=warn"),
    )
    .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            media_keys::register(app.handle());
            Ok(())
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            // auth
            commands::get_auth_state,
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
