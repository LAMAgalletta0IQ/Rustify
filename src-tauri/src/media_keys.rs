//! System media-key handling (Play/Pause, Next, Previous).
//!
//! Registered from Rust rather than the webview, so the keys work even when
//! the window is unfocused or minimised — which is the whole point.
//!
//! Registration is best-effort: media keys are a global, exclusive resource,
//! so if another running player already holds them we log and carry on rather
//! than failing startup.

use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

const KEYS: [(Code, Action); 3] = [
    (Code::MediaPlayPause, Action::PlayPause),
    (Code::MediaTrackNext, Action::Next),
    (Code::MediaTrackPrevious, Action::Prev),
];

#[derive(Clone, Copy)]
enum Action {
    PlayPause,
    Next,
    Prev,
}

pub fn register(app: &AppHandle) {
    for (code, action) in KEYS {
        let shortcut = Shortcut::new(None, code);
        let handle = app.clone();

        let res = app
            .global_shortcut()
            .on_shortcut(shortcut, move |_, _, event| {
                // Fire once per press, not again on release.
                if event.state() != ShortcutState::Pressed {
                    return;
                }
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    let guard = state.spotify.read().await;
                    let Some(session) = guard.as_ref() else {
                        return;
                    };
                    let result = match action {
                        Action::PlayPause => session.spirc.play_pause(),
                        Action::Next => session.spirc.next(),
                        Action::Prev => session.spirc.prev(),
                    };
                    if let Err(e) = result {
                        log::warn!("media key command failed: {e}");
                    }
                });
            });

        if let Err(e) = res {
            log::warn!("could not register media key {code:?} (another player may hold it): {e}");
        }
    }
}
