//! Cancellable sleep timer driven by monotonic time and real player events.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::state::{events, AppState, PlaybackState};

const MAX_DURATION_SECONDS: u64 = 7 * 24 * 60 * 60;
const PAUSE_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SleepTimerMode {
    Duration,
    EndOfTrack,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SleepTimerStatus {
    pub active: bool,
    pub mode: Option<SleepTimerMode>,
    pub ends_at_unix_ms: Option<u64>,
    pub remaining_seconds: Option<u64>,
}

#[derive(Default)]
struct Inner {
    mode: Option<SleepTimerMode>,
    deadline: Option<Instant>,
    ends_at_unix_ms: Option<u64>,
    generation: u64,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
}

#[derive(Default)]
pub struct SleepTimerController {
    inner: tokio::sync::Mutex<Inner>,
}

impl SleepTimerController {
    pub async fn status(&self) -> SleepTimerStatus {
        let inner = self.inner.lock().await;
        status(&inner)
    }

    pub async fn start_duration(
        &self,
        app: AppHandle,
        seconds: u64,
    ) -> AppResult<SleepTimerStatus> {
        if seconds == 0 || seconds > MAX_DURATION_SECONDS {
            return Err(AppError::BadRequest(
                "sleep timer duration must be between 1 second and 7 days".to_string(),
            ));
        }
        let duration = Duration::from_secs(seconds);
        let deadline = Instant::now() + duration;
        let ends_at_unix_ms = unix_ms().saturating_add(duration.as_millis() as u64);
        let mut inner = self.inner.lock().await;
        abort_task(&mut inner);
        inner.generation = inner.generation.wrapping_add(1);
        let generation = inner.generation;
        inner.mode = Some(SleepTimerMode::Duration);
        inner.deadline = Some(deadline);
        inner.ends_at_unix_ms = Some(ends_at_unix_ms);
        inner.task = Some(spawn_deadline(
            app.clone(),
            deadline,
            generation,
            SleepTimerMode::Duration,
        ));
        let result = status(&inner);
        drop(inner);
        emit(&app, &result);
        Ok(result)
    }

    pub async fn start_end_of_track(&self, app: &AppHandle) -> SleepTimerStatus {
        // Snapshot before taking the timer mutex. Player events update
        // playback before notifying this controller, so the opposite lock
        // order could deadlock a command racing a `Playing` event.
        let playback = crate::player::snapshot(&app.state::<AppState>()).await;
        let mut inner = self.inner.lock().await;
        abort_task(&mut inner);
        inner.generation = inner.generation.wrapping_add(1);
        inner.mode = Some(SleepTimerMode::EndOfTrack);
        inner.deadline = None;
        inner.ends_at_unix_ms = None;
        arm_remote_end_of_track(&mut inner, app, &playback);
        let result = status(&inner);
        drop(inner);
        emit(app, &result);
        result
    }

    pub async fn cancel(&self, app: Option<&AppHandle>) -> SleepTimerStatus {
        let mut inner = self.inner.lock().await;
        abort_task(&mut inner);
        inner.generation = inner.generation.wrapping_add(1);
        clear(&mut inner);
        let result = status(&inner);
        drop(inner);
        if let Some(app) = app {
            emit(app, &result);
        }
        result
    }

    /// Called only from librespot's actual `EndOfTrack` event, so skips and
    /// context replacements do not accidentally trigger this mode.
    pub async fn on_end_of_track(&self, app: &AppHandle) {
        let generation = self.inner.lock().await.generation;
        if self.fire(generation, SleepTimerMode::EndOfTrack).await {
            if let Err(error) = pause_active_playback_with_retry(app).await {
                log::warn!("spotify.player: end-of-track sleep timer could not pause: {error}");
            }
            emit(app, &SleepTimerStatus::default());
        }
    }

    /// Keeps end-of-track timers useful while this app controls another
    /// Connect device. The Web API does not expose the private
    /// `set_sleep_timer` command, so each authoritative remote snapshot moves
    /// a monotonic deadline to that device's reported track end. Local
    /// playback still uses the exact librespot event above.
    pub async fn observe_remote_playback(&self, app: &AppHandle, playback: &PlaybackState) {
        let mut inner = self.inner.lock().await;
        if inner.mode != Some(SleepTimerMode::EndOfTrack) || playback.is_active_device {
            return;
        }
        abort_task(&mut inner);
        inner.generation = inner.generation.wrapping_add(1);
        inner.deadline = None;
        arm_remote_end_of_track(&mut inner, app, playback);
    }

    /// A remote estimate must never survive a transfer back to this device;
    /// the subsequent local `EndOfTrack` event becomes authoritative.
    pub async fn observe_local_playback(&self) {
        let mut inner = self.inner.lock().await;
        if inner.mode == Some(SleepTimerMode::EndOfTrack) && inner.deadline.is_some() {
            abort_task(&mut inner);
            inner.generation = inner.generation.wrapping_add(1);
            inner.deadline = None;
        }
    }

    async fn fire(&self, generation: u64, expected: SleepTimerMode) -> bool {
        let mut inner = self.inner.lock().await;
        if inner.generation != generation || inner.mode != Some(expected) {
            return false;
        }
        // Taking the handle from inside its own future merely detaches an
        // already-running task. Replacement/cancel uses `abort_task` instead.
        inner.task.take();
        clear(&mut inner);
        true
    }
}

fn status(inner: &Inner) -> SleepTimerStatus {
    let remaining_seconds = (inner.mode == Some(SleepTimerMode::Duration))
        .then_some(inner.deadline)
        .flatten()
        .map(|deadline| {
            let millis = deadline
                .saturating_duration_since(Instant::now())
                .as_millis() as u64;
            millis.div_ceil(1000)
        });
    SleepTimerStatus {
        active: inner.mode.is_some(),
        mode: inner.mode,
        ends_at_unix_ms: inner.ends_at_unix_ms,
        remaining_seconds,
    }
}

fn clear(inner: &mut Inner) {
    inner.mode = None;
    inner.deadline = None;
    inner.ends_at_unix_ms = None;
}

fn abort_task(inner: &mut Inner) {
    if let Some(task) = inner.task.take() {
        task.abort();
    }
}

fn emit(app: &AppHandle, status: &SleepTimerStatus) {
    if let Err(error) = app.emit(events::SLEEP_TIMER, status) {
        log::warn!("spotify.player: sleep timer event failed: {error}");
    }
}

fn spawn_deadline(
    app: AppHandle,
    deadline: Instant,
    generation: u64,
    mode: SleepTimerMode,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep_until(deadline.into()).await;
        let state = app.state::<AppState>();
        if state.sleep_timer.fire(generation, mode).await {
            if let Err(error) = pause_active_playback_with_retry(&app).await {
                log::warn!("spotify.player: {mode:?} sleep timer could not pause: {error}");
            }
            emit(&app, &SleepTimerStatus::default());
        }
    })
}

fn arm_remote_end_of_track(inner: &mut Inner, app: &AppHandle, playback: &PlaybackState) {
    let Some(remaining) = remote_track_remaining(playback) else {
        return;
    };
    let deadline = Instant::now() + remaining;
    inner.deadline = Some(deadline);
    inner.task = Some(spawn_deadline(
        app.clone(),
        deadline,
        inner.generation,
        SleepTimerMode::EndOfTrack,
    ));
}

fn remote_track_remaining(playback: &PlaybackState) -> Option<Duration> {
    if playback.is_active_device || !playback.is_playing || playback.track.is_none() {
        return None;
    }
    let remaining_ms = playback.duration_ms.saturating_sub(playback.position_ms);
    (remaining_ms > 0).then(|| Duration::from_millis(u64::from(remaining_ms)))
}

async fn pause_active_playback_with_retry(app: &AppHandle) -> AppResult<()> {
    let mut last_error = None;
    for attempt in 0..PAUSE_ATTEMPTS {
        match pause_active_playback(app).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < PAUSE_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::Other("sleep timer pause failed".to_string())))
}

async fn pause_active_playback(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    if state.playback.read().await.is_active_device {
        let spotify = state.spotify.read().await;
        let session = spotify.as_ref().ok_or(AppError::NotLoggedIn)?;
        session.spirc.pause().map_err(AppError::from)
    } else {
        let token = state.tokens.get().await;
        if token.is_empty() {
            return Err(AppError::NotLoggedIn);
        }
        state
            .web_api
            .put_query(&token, "/me/player/pause", &[])
            .await
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replacement_and_cancel_invalidate_old_generation() {
        let timer = SleepTimerController::default();
        {
            let mut inner = timer.inner.lock().await;
            inner.mode = Some(SleepTimerMode::Duration);
            inner.generation = 4;
            inner.deadline = Some(Instant::now() + Duration::from_secs(30));
        }
        assert!(!timer.fire(3, SleepTimerMode::Duration).await);
        assert!(timer.status().await.active);
        assert!(timer.fire(4, SleepTimerMode::Duration).await);
        assert!(!timer.status().await.active);
    }

    #[test]
    fn remaining_seconds_stays_within_deadline_bounds() {
        let inner = Inner {
            mode: Some(SleepTimerMode::Duration),
            deadline: Some(Instant::now() + Duration::from_secs(10)),
            ..Default::default()
        };
        let remaining = status(&inner).remaining_seconds.expect("remaining");
        assert!((9..=10).contains(&remaining));
    }

    #[test]
    fn remote_end_of_track_deadline_is_not_exposed_as_duration() {
        let inner = Inner {
            mode: Some(SleepTimerMode::EndOfTrack),
            deadline: Some(Instant::now() + Duration::from_secs(10)),
            ..Default::default()
        };
        let status = status(&inner);
        assert!(status.active);
        assert_eq!(status.mode, Some(SleepTimerMode::EndOfTrack));
        assert_eq!(status.remaining_seconds, None);
        assert_eq!(status.ends_at_unix_ms, None);
    }

    #[test]
    fn remote_deadline_uses_reported_track_remainder_only_while_playing() {
        let mut playback = PlaybackState {
            is_playing: true,
            duration_ms: 180_000,
            position_ms: 42_000,
            track: Some(crate::state::TrackInfo {
                uri: "spotify:track:0123456789012345678901".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            remote_track_remaining(&playback),
            Some(Duration::from_secs(138))
        );
        playback.is_playing = false;
        assert_eq!(remote_track_remaining(&playback), None);
        playback.is_playing = true;
        playback.is_active_device = true;
        assert_eq!(remote_track_remaining(&playback), None);
    }
}
