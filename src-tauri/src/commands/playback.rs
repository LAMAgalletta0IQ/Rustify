//! Transport controls, Spotify Connect device management, the sleep timer,
//! context/track loading, and the local queue projection.

use librespot::connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auth;
use crate::connect::{self, Device};
use crate::error::{AppError, AppResult};
use crate::player;
use crate::queue::{self, QueueView};
use crate::state::{events, AppState, PlaybackState};

use super::{
    clear_crossfade_before_transition, configured_crossfade, remote_post, remote_put, token,
    with_spirc,
};

#[tauri::command]
pub async fn get_playback(state: State<'_, AppState>) -> AppResult<PlaybackState> {
    Ok(player::snapshot(&state).await)
}

#[tauri::command]
pub async fn play(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.play()).await
    } else {
        remote_put(&state, "/me/player/play", &[]).await
    }
}

#[tauri::command]
pub async fn pause(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.pause()).await
    } else {
        remote_put(&state, "/me/player/pause", &[]).await
    }
}

#[tauri::command]
pub async fn play_pause(state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.play_pause()).await
    } else if state.playback.read().await.is_playing {
        remote_put(&state, "/me/player/pause", &[]).await
    } else {
        remote_put(&state, "/me/player/play", &[]).await
    }
}

#[tauri::command]
pub async fn next_track(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        let resume = clear_crossfade_before_transition(&app, &state).await?;
        let transition = with_spirc(&state, |s| s.next()).await;
        if resume {
            let resumed = with_spirc(&state, |s| s.play()).await;
            transition.and(resumed)
        } else {
            transition
        }
    } else {
        remote_post(&state, "/me/player/next", &[]).await
    }
}

#[tauri::command]
pub async fn previous_track(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        let resume = clear_crossfade_before_transition(&app, &state).await?;
        let transition = with_spirc(&state, |s| s.prev()).await;
        if resume {
            let resumed = with_spirc(&state, |s| s.play()).await;
            transition.and(resumed)
        } else {
            transition
        }
    } else {
        remote_post(&state, "/me/player/previous", &[]).await
    }
}

#[tauri::command]
pub async fn seek(app: AppHandle, state: State<'_, AppState>, position_ms: u32) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        let was_playing = state.playback.read().await.is_playing;
        let crossfade_enabled = configured_crossfade(&app);

        // The reviewed librespot PR clears an in-flight fade on Pause but not
        // on Seek. Bracket an explicit local seek so an outgoing decoder can
        // never continue underneath audio from the new position. Preserve a
        // user-paused state by only resuming when playback was running.
        if crossfade_enabled && was_playing {
            with_spirc(&state, |s| s.pause()).await?;
            let seek_result = with_spirc(&state, |s| s.set_position_ms(position_ms)).await;
            let resume_result = with_spirc(&state, |s| s.play()).await;
            seek_result.and(resume_result)
        } else {
            with_spirc(&state, |s| s.set_position_ms(position_ms)).await
        }
    } else {
        remote_put(
            &state,
            "/me/player/seek",
            &[("position_ms", position_ms.to_string())],
        )
        .await
    }
}

/// `percent` is 0..=100; librespot's own scale is 0..=65535.
#[tauri::command]
pub async fn set_volume(app: AppHandle, state: State<'_, AppState>, percent: u8) -> AppResult<()> {
    if percent > 100 {
        return Err(AppError::BadRequest(
            "Volume must be between 0 and 100.".into(),
        ));
    }
    if state.playback.read().await.is_active_device {
        let v = player::percent_to_volume(percent);
        with_spirc(&state, |s| s.set_volume(v)).await?;
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;
        let mut settings = auth::settings_or_default(&data_dir);
        settings.last_volume_percent = percent;
        auth::save_settings(&data_dir, &settings)
    } else {
        remote_put(
            &state,
            "/me/player/volume",
            &[("volume_percent", percent.to_string())],
        )
        .await
    }
}

#[tauri::command]
pub async fn set_shuffle(state: State<'_, AppState>, shuffle: bool) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.shuffle(shuffle)).await
    } else {
        remote_put(
            &state,
            "/me/player/shuffle",
            &[("state", shuffle.to_string())],
        )
        .await
    }
}

#[tauri::command]
pub async fn set_repeat(state: State<'_, AppState>, context: bool, track: bool) -> AppResult<()> {
    if state.playback.read().await.is_active_device {
        with_spirc(&state, |s| s.repeat(context)).await?;
        with_spirc(&state, |s| s.repeat_track(track)).await
    } else {
        let repeat = if track {
            "track"
        } else if context {
            "context"
        } else {
            "off"
        };
        remote_put(
            &state,
            "/me/player/repeat",
            &[("state", repeat.to_string())],
        )
        .await
    }
}

#[tauri::command]
pub async fn get_sleep_timer(
    state: State<'_, AppState>,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    Ok(state.sleep_timer.status().await)
}

#[tauri::command]
pub async fn start_sleep_timer(
    app: AppHandle,
    state: State<'_, AppState>,
    seconds: u64,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    state.sleep_timer.start_duration(app, seconds).await
}

#[tauri::command]
pub async fn sleep_at_end_of_track(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    Ok(state.sleep_timer.start_end_of_track(&app).await)
}

#[tauri::command]
pub async fn cancel_sleep_timer(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<crate::sleep_timer::SleepTimerStatus> {
    Ok(state.sleep_timer.cancel(Some(&app)).await)
}

/// Loads a *context* (playlist, album, artist, or the liked-songs collection)
/// and optionally starts at a specific track within it.
///
/// This is the path that matters for playback feeling correct: loading a bare
/// track URI plays that one track and stops, whereas loading the containing
/// context lets playback continue through the rest of it.
#[tauri::command]
pub async fn load_context(
    app: AppHandle,
    state: State<'_, AppState>,
    context_uri: String,
    track_uri: Option<String>,
) -> AppResult<()> {
    // Taking over playback is the point of an explicit load, so activate here
    // rather than at login — see the note in `player::start_session`. Skipped
    // when already active, which librespot would otherwise log as
    // "SpircCommand::Activate will be ignored while already active".
    let activate = !state.playback.read().await.is_active_device;

    let requested_episode = track_uri
        .as_deref()
        .filter(|uri| uri.starts_with("spotify:episode:"))
        .or_else(|| {
            context_uri
                .starts_with("spotify:episode:")
                .then_some(context_uri.as_str())
        });
    let seek_to = if let Some(uri) = requested_episode {
        let session = state
            .spotify
            .read()
            .await
            .as_ref()
            .map(|spotify| spotify.session.clone())
            .ok_or(AppError::NotLoggedIn)?;
        match crate::podcasts::get(&session, uri).await {
            Ok(resume) if !resume.completed => resume.position_ms.min(u64::from(u32::MAX)) as u32,
            Ok(_) => 0,
            Err(error) => {
                log::debug!(target: "spotify.podcasts", "episode resume lookup failed safely: {error}");
                0
            }
        }
    } else {
        0
    };

    let resume_on_error = clear_crossfade_before_transition(&app, &state).await?;
    let load_result = with_spirc(&state, move |s| {
        if activate {
            s.activate()?;
        }
        s.load(LoadRequest::from_context_uri(
            context_uri,
            LoadRequestOptions {
                start_playing: true,
                seek_to,
                playing_track: track_uri.map(PlayingTrack::Uri),
                ..Default::default()
            },
        ))
    })
    .await;
    if load_result.is_err() && resume_on_error {
        let _ = with_spirc(&state, |spirc| spirc.play()).await;
    }
    load_result
}

/// Loads an explicit list of track URIs as an ad-hoc context.
///
/// Used where there is no container to play from — search results, or picking
/// an entry out of the queue.
#[tauri::command]
pub async fn load_tracks(
    app: AppHandle,
    state: State<'_, AppState>,
    uris: Vec<String>,
    start_uri: Option<String>,
) -> AppResult<()> {
    if uris.is_empty() {
        return Ok(());
    }
    let activate = !state.playback.read().await.is_active_device;

    let requested_episode = start_uri
        .as_deref()
        .or_else(|| uris.first().map(String::as_str))
        .filter(|uri| uri.starts_with("spotify:episode:"));
    let seek_to = if let Some(uri) = requested_episode {
        let session = state
            .spotify
            .read()
            .await
            .as_ref()
            .map(|spotify| spotify.session.clone())
            .ok_or(AppError::NotLoggedIn)?;
        match crate::podcasts::get(&session, uri).await {
            Ok(resume) if !resume.completed => resume.position_ms.min(u64::from(u32::MAX)) as u32,
            Ok(_) => 0,
            Err(error) => {
                log::debug!(target: "spotify.podcasts", "episode resume lookup failed safely: {error}");
                0
            }
        }
    } else {
        0
    };

    let resume_on_error = clear_crossfade_before_transition(&app, &state).await?;
    let load_result = with_spirc(&state, move |s| {
        if activate {
            s.activate()?;
        }
        s.load(LoadRequest::from_tracks(
            uris,
            LoadRequestOptions {
                start_playing: true,
                seek_to,
                playing_track: start_uri.map(PlayingTrack::Uri),
                ..Default::default()
            },
        ))
    })
    .await;
    if load_result.is_err() && resume_on_error {
        let _ = with_spirc(&state, |spirc| spirc.play()).await;
    }
    load_result
}

// ---- connect ------------------------------------------------------------

#[tauri::command]
pub async fn list_devices(state: State<'_, AppState>) -> AppResult<Vec<Device>> {
    let t = token(&state).await?;
    connect::list_devices(&state.web_api, &t).await
}

#[tauri::command]
pub async fn transfer_playback(
    state: State<'_, AppState>,
    device_id: String,
    play: bool,
) -> AppResult<()> {
    let t = token(&state).await?;
    connect::transfer_playback(&state.web_api, &t, &device_id, play).await
}

/// Pulls playback back to this app by activating our own Spirc device.
#[tauri::command]
pub async fn activate_this_device(state: State<'_, AppState>) -> AppResult<()> {
    with_spirc(&state, |s| s.activate()).await
}

// ---- queue ----------------------------------------------------------------

#[tauri::command]
pub async fn get_queue(state: State<'_, AppState>) -> AppResult<QueueView> {
    let t = token(&state).await?;
    let web = queue::get_queue(&state.web_api, &t).await?;
    let merged = queue::merge_metadata(&*state.queue.read().await, web);
    *state.queue.write().await = merged.clone();
    Ok(merged)
}

#[tauri::command]
pub async fn add_to_queue(
    app: AppHandle,
    state: State<'_, AppState>,
    uri: String,
) -> AppResult<()> {
    queue::validate_queue_uri(&uri)?;
    let local = state.playback.read().await.is_active_device;
    let t = token(&state).await?;
    let api = &state.web_api;
    if local {
        let parsed = librespot::core::SpotifyUri::from_uri(&uri)
            .map_err(|error| AppError::BadRequest(format!("invalid queue URI: {error}")))?;
        with_spirc(&state, move |spirc| spirc.add_to_queue(parsed)).await?;
    } else {
        queue::add_to_queue(api, &t, &uri).await?;
    }
    if let Ok(web) = queue::get_queue(api, &t).await {
        let merged = queue::merge_metadata(&*state.queue.read().await, web);
        *state.queue.write().await = merged.clone();
        let _ = app.emit(events::QUEUE, merged);
    }
    Ok(())
}
