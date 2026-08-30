//! Experimental Spotify Jams (social-connect v2) commands. See
//! `README_jams.md` and `crate::jams_bridge`.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::error::{AppError, AppResult};
use crate::jams::JamSession;
use crate::jams_bridge::JamController;
use crate::state::AppState;

/// Ensures exactly one `T` is ever built behind `lock`, even when two callers
/// race: a cheap read-locked fast path for the already-built case, then a
/// second check *under the write lock*, with `build` awaited while that write
/// lock is still held. That is the part the naive "check, build, then
/// store-if-still-empty" version got wrong — building outside any lock let
/// two racing callers both pass the `None` check and each build (and, for
/// [`JamController`], each leak dealer subscriptions and an orphan forward
/// task). Holding the write guard across `build` serialises the rare, non-hot
/// jam-controller construction instead, which is the desired behaviour here,
/// not an accepted cost. Generic and unit-tested in isolation below;
/// `ensure_jams` is the sole caller.
async fn get_or_build<T, E>(
    lock: &tokio::sync::RwLock<Option<Arc<T>>>,
    build: impl std::future::Future<Output = Result<T, E>>,
) -> Result<Arc<T>, E> {
    if let Some(existing) = lock.read().await.as_ref() {
        return Ok(existing.clone());
    }
    let mut guard = lock.write().await;
    if let Some(existing) = guard.as_ref() {
        return Ok(existing.clone());
    }
    let built = Arc::new(build.await?);
    guard.replace(built.clone());
    Ok(built)
}

/// Returns the controller, building it lazily. Requires a live session (the
/// jams module needs the user's bearer token), but the check also keeps a
/// logged-out app from spawning a dealer listener that would just reconnect
/// forever with no token.
async fn ensure_jams(app: &AppHandle, state: &AppState) -> AppResult<Arc<JamController>> {
    // Fetched — and the read guard dropped — *before* touching `state.jams`,
    // so this function's lock order (spotify, released, then jams) can never
    // invert against `logout`'s (spotify held across teardown, then jams). If
    // this instead held both at once in the opposite order, ensure_jams and
    // logout could deadlock on each other's lock.
    let session = {
        let guard = state.spotify.read().await;
        let Some(session) = guard.as_ref() else {
            return Err(AppError::NotLoggedIn);
        };
        session.session.clone()
    };
    // Jams hang off the librespot session (device id, client-token, dealer
    // connection id), so the controller cannot be built before it exists.
    get_or_build(
        &state.jams,
        JamController::build(app, session, &state.tokens),
    )
    .await
}

/// Configuration + current session, so the Jams view can explain what still
/// needs capturing (endpoints/hashes) without firing a request.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JamStatus {
    pub spclient_endpoints: usize,
    pub pathfinder_hashes: usize,
    pub session: Option<JamSession>,
}

#[tauri::command]
pub async fn get_jam_status(app: AppHandle, state: State<'_, AppState>) -> AppResult<JamStatus> {
    // Not logged in is not an error here: the view only renders while logged
    // in, but a reload can race that, and a status read must not spawn a
    // dealer loop on its own.
    let Ok(ctrl) = ensure_jams(&app, &state).await else {
        return Ok(JamStatus {
            spclient_endpoints: 0,
            pathfinder_hashes: 0,
            session: None,
        });
    };
    let config = ctrl.config();
    let mut session = ctrl.session().await;
    if let Some(session) = &mut session {
        let queue = state.queue.read().await;
        session.queue = queue
            .queue
            .iter()
            .chain(queue.autoplay.iter())
            .map(|track| crate::jams::JamTrack {
                uri: track.uri.clone(),
                name: Some(track.name.clone()),
                artists: track.artists.clone(),
                added_by: None,
                added_at: None,
            })
            .collect();
    }
    Ok(JamStatus {
        spclient_endpoints: config.spclient_endpoints.len(),
        pathfinder_hashes: config.pathfinder_hashes.len(),
        session,
    })
}

#[tauri::command]
pub async fn create_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.create(None).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn refresh_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.refresh_session().await.map_err(AppError::from)
}

#[tauri::command]
pub async fn join_jam(
    app: AppHandle,
    state: State<'_, AppState>,
    jam_id: String,
) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.join(&jam_id).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn leave_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.leave().await.map_err(AppError::from)
}

/// Adds the currently playing track to the active jam.
#[tauri::command]
pub async fn add_track_to_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    let Some(track) = state.playback.read().await.track.clone() else {
        return Err(AppError::Playback("Nothing is playing.".to_string()));
    };
    ctrl.add_track(&track.uri).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn set_jam_queue_control(
    app: AppHandle,
    state: State<'_, AppState>,
    allowed: bool,
) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.set_queue_control(allowed)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
pub async fn kick_jam_member(
    app: AppHandle,
    state: State<'_, AppState>,
    member_id: String,
) -> AppResult<JamSession> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.kick(&member_id).await.map_err(AppError::from)
}

#[tauri::command]
pub async fn end_jam(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let ctrl = ensure_jams(&app, &state).await?;
    ctrl.sync_token(&state.tokens).await;
    ctrl.end().await.map_err(AppError::from)
}

#[cfg(test)]
mod get_or_build_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Regression test for the `ensure_jams` race (two concurrent jam
    /// commands each building — and leaking — a `JamController`, per
    /// CLAUDE.md's jams-reliability notes): fires many concurrent
    /// `get_or_build` calls against one empty lock and asserts the build
    /// future only ever ran once.
    #[tokio::test]
    async fn concurrent_callers_build_exactly_once() {
        let lock: Arc<tokio::sync::RwLock<Option<Arc<u32>>>> =
            Arc::new(tokio::sync::RwLock::new(None));
        let build_count = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..16 {
            let lock = lock.clone();
            let build_count = build_count.clone();
            handles.push(tokio::spawn(async move {
                get_or_build::<u32, ()>(&lock, async {
                    build_count.fetch_add(1, Ordering::SeqCst);
                    // Widen the race window so concurrent callers actually
                    // overlap instead of trivially serialising through Tokio.
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(7)
                })
                .await
            }));
        }

        for handle in handles {
            assert_eq!(handle.await.unwrap().unwrap().as_ref(), &7);
        }
        assert_eq!(build_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn already_built_value_short_circuits_without_building() {
        let lock: Arc<tokio::sync::RwLock<Option<Arc<u32>>>> =
            Arc::new(tokio::sync::RwLock::new(Some(Arc::new(99))));
        let build_count = Arc::new(AtomicUsize::new(0));
        let build_count_clone = build_count.clone();

        let result = get_or_build::<u32, ()>(&lock, async move {
            build_count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        })
        .await
        .unwrap();

        assert_eq!(*result, 99);
        assert_eq!(build_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn build_failure_leaves_lock_empty_for_a_later_retry() {
        let lock: Arc<tokio::sync::RwLock<Option<Arc<u32>>>> =
            Arc::new(tokio::sync::RwLock::new(None));

        let err: Result<Arc<u32>, &'static str> =
            get_or_build(&lock, async { Err("build failed") }).await;
        assert_eq!(err, Err("build failed"));
        assert!(lock.read().await.is_none());

        let ok = get_or_build::<u32, &'static str>(&lock, async { Ok(42) })
            .await
            .unwrap();
        assert_eq!(*ok, 42);
    }
}
