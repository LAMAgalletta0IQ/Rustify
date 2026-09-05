//! DJ, Home, Listening DNA, lyrics, track credits, episode resume,
//! telemetry, music-video/audio capability, friend presence, profiles and
//! user search — mostly private-API-backed extras layered on top of the
//! core Web API library surface in `library.rs`.

use librespot::connect::{LoadRequest, LoadRequestOptions, PlayingTrack};
use tauri::{AppHandle, Manager, State};

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::lyrics::LyricsResult;
use crate::spotify::{DjSession, HomeFeed};
use crate::state::AppState;

use super::{clear_crossfade_before_transition, token, with_spirc};

/// Whether the optional Last.fm enrichment is configured.
///
/// The key is returned so Settings can show and edit what is in use. It is
/// stored in plaintext in `settings.json` — this is a local desktop app with
/// no key store, and the Spotify refresh tokens next to it are strictly more
/// sensitive. The UI masks the field by default all the same, because Last.fm
/// asks that application keys not be shared.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastfmConfig {
    pub configured: bool,
    pub api_key: Option<String>,
}

#[tauri::command]
pub fn get_lastfm_config(app: AppHandle) -> AppResult<LastfmConfig> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;
    let api_key = auth::lastfm_api_key(&data_dir);
    Ok(LastfmConfig {
        configured: api_key.is_some(),
        api_key,
    })
}

/// Saves the optional Last.fm API key. Requires no re-login: it is unrelated
/// to the Spotify OAuth grant and is only ever used for unauthenticated
/// `artist.getTopTags` reads.
#[tauri::command]
pub async fn set_lastfm_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    api_key: String,
) -> AppResult<()> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "Last.fm API key cannot be empty. Use Clear to remove it instead.".into(),
        ));
    }
    let mut settings = auth::settings_or_default(&data_dir);
    settings.lastfm_api_key = Some(trimmed.to_string());
    auth::save_settings(&data_dir, &settings)?;
    // The cache holds answers obtained with the previous key; a new key may
    // get different ones, and a previously-rejected key's empty results must
    // not survive being corrected.
    state.lastfm_cache.clear().await;
    Ok(())
}

#[tauri::command]
pub async fn clear_lastfm_api_key(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Other(format!("no app data dir: {e}")))?;

    let mut settings = auth::settings_or_default(&data_dir);
    settings.lastfm_api_key = None;
    auth::save_settings(&data_dir, &settings)?;
    state.lastfm_cache.clear().await;
    Ok(())
}

/// Taste profile for the Profile view's radar chart.
///
/// `currentYear` comes from the webview rather than the system clock so the
/// "released in the last two years" axis matches the user's own calendar
/// rather than UTC's.
///
/// Last.fm enrichment is attached only when the user configured a key; with
/// none, the call is exactly what it was before the option existed.
#[tauri::command]
pub async fn get_listening_dna(
    app: AppHandle,
    state: State<'_, AppState>,
    current_year: Option<i32>,
) -> AppResult<crate::dna::ListeningDna> {
    let t = token(&state).await?;
    let year = current_year.unwrap_or(2026);
    let api_key = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|dir| auth::lastfm_api_key(&dir));
    let lastfm = api_key
        .as_deref()
        .map(|api_key| crate::dna::LastfmEnrichment {
            http: &state.lastfm_http,
            cache: &state.lastfm_cache,
            api_key,
        });
    crate::dna::listening_dna(&state.web_api, &t, year, lastfm).await
}

#[tauri::command]
pub async fn get_personalized_home(
    state: State<'_, AppState>,
    limit: Option<u32>,
    time_zone: Option<String>,
) -> AppResult<HomeFeed> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    state
        .internal_spotify
        .home(
            &session,
            limit.unwrap_or(10),
            time_zone.as_deref().unwrap_or("UTC"),
        )
        .await
}

#[tauri::command]
pub async fn get_dj_status(
    state: State<'_, AppState>,
    refresh: Option<bool>,
) -> AppResult<Option<DjSession>> {
    if !refresh.unwrap_or(false) {
        if let Some(session) = state.internal_spotify.cached_dj().await {
            return Ok(Some(session));
        }
    }
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    match state
        .internal_spotify
        .resolve_dj(&session, refresh.unwrap_or(false))
        .await
    {
        Ok(session) => Ok(Some(session)),
        // Not an error state as far as the Home card is concerned: the DJ
        // button still works, it just plays the public playlist. Returning
        // Err here left the button permanently disabled with a message the
        // user could do nothing about.
        Err(AppError::LexiconUnavailable(reason)) => {
            log::debug!(target: "spotify.dj", "Lexicon unavailable ({reason}); reporting fallback session");
            Ok(Some(crate::spotify::dj::fallback_session()))
        }
        Err(error) => Err(error),
    }
}

/// Resolves the dynamic DJ session through Lexicon before loading its current
/// track window. librespot 0.8 cannot resolve empty dynamic context pages on
/// its own, so passing the recovered URIs is the compatibility path.
#[tauri::command]
pub async fn start_dj(app: AppHandle, state: State<'_, AppState>) -> AppResult<DjSession> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    // A fresh session needs the full state_restore metadata (volatile context,
    // Lexicon clock and session-control fields). The small interactive window
    // is appropriate only for later queue replenishment.
    //
    // Lexicon is the one endpoint here Spotify gates hardest at non-official
    // clients: it answers 403/404 for most accounts, which used to make the DJ
    // button a permanent error message. When that happens, fall back to
    // playing the canonical public DJ playlist as an ordinary context. That
    // loses the dynamic re-resolution and the spoken intros (the public
    // playlist carries no narration metadata to resolve, regardless of
    // `narration_playback_supported`) — but it does play the DJ mix, which is
    // what the button says it will do.
    let mut dj = match state.internal_spotify.resolve_dj(&session, true).await {
        Ok(dj) => dj,
        Err(AppError::LexiconUnavailable(reason)) => {
            log::info!(
                target: "spotify.dj",
                "Lexicon unavailable ({reason}); falling back to the public DJ playlist"
            );
            return start_dj_fallback(&app, &state).await;
        }
        Err(error) => return Err(error),
    };
    match state
        .internal_spotify
        .prepare_dj_narration(&session, &dj)
        .await
    {
        Ok(resolved) => dj.narration_resolved = resolved,
        Err(error) => {
            log::warn!(target: "spotify.dj", "narration preflight failed; continuing with music: {error}")
        }
    }

    // Only the first track is loaded here; `player::spawn_dj_advance` takes
    // over from `PlayerEvent::EndOfTrack` onward, loading one DJ track at a
    // time so an intro/outro clip can play in the gap between them. See its
    // doc comment for why DJ tracks are no longer queued into Spirc upfront.
    let Some(first_track) = dj.tracks.first().cloned() else {
        return Err(AppError::LexiconUnavailable(
            "Lexicon returned no playable DJ tracks".into(),
        ));
    };
    crate::player::play_dj_intro(&app, &session, &first_track).await;

    let first_uri = first_track.uri.clone();
    let activate = !state.playback.read().await.is_active_device;
    let resume_on_error = clear_crossfade_before_transition(&app, &state).await?;
    let load_result = with_spirc(&state, move |spirc| {
        if activate {
            spirc.activate()?;
        }
        spirc.load(LoadRequest::from_tracks(
            vec![first_uri.clone()],
            LoadRequestOptions {
                start_playing: true,
                playing_track: Some(PlayingTrack::Uri(first_uri)),
                ..Default::default()
            },
        ))
    })
    .await;
    if load_result.is_err() && resume_on_error {
        let _ = with_spirc(&state, |spirc| spirc.play()).await;
    }
    load_result?;
    state
        .internal_spotify
        .dj
        .update_status(true, dj.narration_resolved)
        .await;
    dj.active = true;
    Ok(dj)
}

/// Plays Spotify's public DJ playlist as a plain context, for accounts where
/// Lexicon refuses to resolve a dynamic session.
///
/// Returns a `DjSession` describing exactly what the user got — `available` is
/// true (something is playing) but `dynamic_refill_supported` and
/// `narration_resolved` are false, and `reason` carries the fallback marker
/// the UI keys its explanatory copy off. Reporting it as a full DJ session
/// would be a lie the UI has no way to detect.
async fn start_dj_fallback(app: &AppHandle, state: &AppState) -> AppResult<DjSession> {
    let session = crate::spotify::dj::fallback_session();
    let context_uri = session.context_uri.clone();
    let activate = !state.playback.read().await.is_active_device;
    let resume_on_error = clear_crossfade_before_transition(app, state).await?;
    let uri = context_uri.clone();
    let load_result = with_spirc(state, move |spirc| {
        if activate {
            spirc.activate()?;
        }
        spirc.load(LoadRequest::from_context_uri(
            uri,
            LoadRequestOptions {
                start_playing: true,
                ..Default::default()
            },
        ))
    })
    .await;
    if load_result.is_err() && resume_on_error {
        let _ = with_spirc(state, |spirc| spirc.play()).await;
    }
    load_result?;

    let session = DjSession {
        active: true,
        ..session
    };
    state.internal_spotify.dj.set_cached(session.clone()).await;
    Ok(session)
}

#[tauri::command]
pub async fn get_track_credits(
    state: State<'_, AppState>,
    track_uri: String,
) -> AppResult<crate::spotify::TrackCredits> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    state
        .internal_spotify
        .track_credits(&session, &track_uri)
        .await
}

#[tauri::command]
pub async fn get_episode_resume(
    state: State<'_, AppState>,
    episode_uri: String,
) -> AppResult<crate::podcasts::EpisodeResume> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    crate::podcasts::get(&session, &episode_uri).await
}

#[tauri::command]
pub async fn set_episode_completed(
    state: State<'_, AppState>,
    episode_uri: String,
    completed: bool,
) -> AppResult<()> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    crate::podcasts::set_completed(&session, &episode_uri, completed).await
}

#[tauri::command]
pub async fn get_telemetry_status(
    state: State<'_, AppState>,
) -> AppResult<crate::telemetry::TelemetryStatus> {
    Ok(state.telemetry.status().await)
}

#[tauri::command]
pub async fn get_music_video_capability(
    state: State<'_, AppState>,
    track_uri: String,
) -> AppResult<crate::music_videos::MusicVideoCapability> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    crate::music_videos::capability(&session, &track_uri).await
}

#[tauri::command]
pub async fn get_audio_capability(
    state: State<'_, AppState>,
    track_uri: String,
) -> AppResult<crate::audio_capabilities::AudioCapability> {
    if let Some(cached) = state
        .audio_capability_cache
        .read()
        .await
        .get(&track_uri)
        .cloned()
    {
        return Ok(cached);
    }
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    let capability = crate::audio_capabilities::inspect(&session, &track_uri).await?;
    let mut cache = state.audio_capability_cache.write().await;
    if cache.len() >= 128 {
        if let Some(key) = cache.keys().next().cloned() {
            cache.remove(&key);
        }
    }
    cache.insert(track_uri, capability.clone());
    Ok(capability)
}

#[tauri::command]
pub async fn get_lyrics(
    state: State<'_, AppState>,
    track_uri: String,
    track_name: String,
    artist_name: String,
    album_name: String,
    duration_ms: u32,
) -> AppResult<LyricsResult> {
    crate::lyrics::validate_track_uri(&track_uri)?;
    if let Some(cached) = state.lyrics_cache.read().await.get(&track_uri).cloned() {
        return Ok(cached);
    }

    const MAX_LYRICS_CACHE_ENTRIES: usize = 64;
    let request_gate = {
        let mut requests = state.lyrics_requests.lock().await;
        if requests.len() >= MAX_LYRICS_CACHE_ENTRIES && !requests.contains_key(&track_uri) {
            requests.clear();
        }
        requests
            .entry(track_uri.clone())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    let _request_guard = request_gate.lock().await;
    if let Some(cached) = state.lyrics_cache.read().await.get(&track_uri).cloned() {
        return Ok(cached);
    }

    // Lyrics are a separate, read-only integration. Requiring an active
    // session prevents stale track details from making background requests
    // after logout.
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    let result = crate::lyrics::fetch(
        &session,
        &track_uri,
        &track_name,
        &artist_name,
        &album_name,
        duration_ms,
    )
    .await?;

    let mut cache = state.lyrics_cache.write().await;
    if cache.len() >= MAX_LYRICS_CACHE_ENTRIES && !cache.contains_key(&track_uri) {
        // Lyrics are immutable enough for a session; clearing at the hard
        // bound is cheap and keeps memory use deterministic without another
        // cache dependency.
        cache.clear();
    }
    cache.insert(track_uri, result.clone());
    Ok(result)
}

#[tauri::command]
pub async fn get_friend_activity(
    state: State<'_, AppState>,
) -> AppResult<crate::friends::FriendFeed> {
    if state.spotify.read().await.is_none() {
        return Err(AppError::NotLoggedIn);
    }
    Ok(state.friend_activity.read().await.clone())
}

#[tauri::command]
pub async fn get_user_profile(
    state: State<'_, AppState>,
    username: Option<String>,
) -> AppResult<crate::profiles::UserProfile> {
    let session = {
        let spotify = state.spotify.read().await;
        spotify
            .as_ref()
            .map(|spotify| spotify.session.clone())
            .ok_or(AppError::NotLoggedIn)?
    };
    let authenticated_username = state.auth.read().await.user_id.clone();
    let username = match username.filter(|value| !value.trim().is_empty()) {
        Some(username) => username,
        None => authenticated_username
            .clone()
            .ok_or_else(|| AppError::Auth("Spotify session has no username".to_string()))?,
    };
    let mut profile = crate::profiles::fetch(&session, &username).await?;
    // Older responses omit `is_current_user`. The authenticated username is
    // authoritative and prevents the UI treating the owner's profile as a
    // public visitor view when that optional response field is absent.
    if authenticated_username.is_some_and(|current| current.eq_ignore_ascii_case(&profile.username))
    {
        profile.is_current_user = true;
    }
    Ok(profile)
}

#[tauri::command]
pub async fn search_users(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<crate::spotify::UserSearchPage> {
    let session = state
        .spotify
        .read()
        .await
        .as_ref()
        .map(|spotify| spotify.session.clone())
        .ok_or(AppError::NotLoggedIn)?;
    state
        .internal_spotify
        .search_users(&session, &query, limit.unwrap_or(12), offset.unwrap_or(0))
        .await
}
