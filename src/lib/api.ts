import { invoke } from "@tauri-apps/api/core";
import type {
  AlbumSummary,
  AlbumPage,
  AppSettings,
  AudioDevice,
  AudioStatus,
  AppErrorPayload,
  ArtistPage,
  FollowedReleasePage,
  HomeFeed,
  ArtistSummary,
  AuthState,
  Device,
  DeviceAuthorization,
  DjSession,
  JamSession,
  JamStatus,
  LoginInfo,
  LyricsResult,
  PlaybackState,
  PlaylistSummary,
  QueueView,
  RecentActivityItem,
  SearchResults,
  SleepTimerStatus,
  TrackSummary,
  EqualizerPreset,
} from "./types";

/** Tauri event names — must match `state::events` in Rust. */
export const EVENT_PLAYBACK = "playback:changed";
export const EVENT_AUTH = "auth:changed";
export const EVENT_JAMS = "jams:changed";
export const EVENT_QUEUE = "queue:changed";
export const EVENT_SLEEP_TIMER = "sleep-timer:changed";

/**
 * Tauri rejects with the serialised `AppError`. Normalise it so callers always
 * get a typed payload instead of a bare unknown.
 */
export function asAppError(e: unknown): AppErrorPayload {
  if (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    "message" in e
  ) {
    return e as AppErrorPayload;
  }
  return { kind: "Other", message: String(e) };
}

// ---- auth ---------------------------------------------------------------
export const getAuthState = () => invoke<AuthState>("get_auth_state");
export const getLoginInfo = () => invoke<LoginInfo>("get_login_info");
export const setClientId = (clientId: string) =>
  invoke<void>("set_client_id", { clientId });
export const login = () => invoke<AuthState>("login");
export const startDeviceAuthorization = () =>
  invoke<DeviceAuthorization>("start_device_authorization");
export const completeDeviceAuthorization = () =>
  invoke<AuthState>("complete_device_authorization");
export const cancelDeviceAuthorization = () =>
  invoke<void>("cancel_device_authorization");
export const restoreSession = () => invoke<AuthState>("restore_session");
export const logout = () => invoke<void>("logout");
export const getSettings = () => invoke<AppSettings>("get_settings");
export const updateSettings = (settings: AppSettings) =>
  invoke<AppSettings>("update_settings", { settings });
export const listAudioDevices = () =>
  invoke<AudioDevice[]>("list_audio_devices");
export const getAudioStatus = () => invoke<AudioStatus>("get_audio_status");
export const getEqualizerPresets = () =>
  invoke<EqualizerPreset[]>("get_equalizer_presets");

// ---- playback -----------------------------------------------------------
export const getPlayback = () => invoke<PlaybackState>("get_playback");
export const play = () => invoke<void>("play");
export const pause = () => invoke<void>("pause");
export const playPause = () => invoke<void>("play_pause");
export const nextTrack = () => invoke<void>("next_track");
export const previousTrack = () => invoke<void>("previous_track");
export const seek = (positionMs: number) =>
  invoke<void>("seek", { positionMs });
export const setVolume = (percent: number) =>
  invoke<void>("set_volume", { percent });
export const setShuffle = (shuffle: boolean) =>
  invoke<void>("set_shuffle", { shuffle });
export const setRepeat = (context: boolean, track: boolean) =>
  invoke<void>("set_repeat", { context, track });
export const getSleepTimer = () => invoke<SleepTimerStatus>("get_sleep_timer");
export const startSleepTimer = (seconds: number) =>
  invoke<SleepTimerStatus>("start_sleep_timer", { seconds });
export const sleepAtEndOfTrack = () =>
  invoke<SleepTimerStatus>("sleep_at_end_of_track");
export const cancelSleepTimer = () =>
  invoke<SleepTimerStatus>("cancel_sleep_timer");
/**
 * Play a context (playlist/album/artist/collection), optionally starting at a
 * specific track so the rest of the context keeps playing after it.
 */
export const loadContext = (contextUri: string, trackUri?: string) =>
  invoke<void>("load_context", { contextUri, trackUri });

/** Play an explicit track list, for results that have no container context. */
export const loadTracks = (uris: string[], startUri?: string) =>
  invoke<void>("load_tracks", { uris, startUri });

/** URI of the account's Liked Songs, which acts as a normal context. */
export const likedSongsUri = (userId: string) =>
  `spotify:user:${userId}:collection`;

// ---- connect ------------------------------------------------------------
export const listDevices = () => invoke<Device[]>("list_devices");
export const transferPlayback = (deviceId: string, play: boolean) =>
  invoke<void>("transfer_playback", { deviceId, play });
export const activateThisDevice = () => invoke<void>("activate_this_device");

// ---- library ------------------------------------------------------------
export const getPlaylists = (limit?: number, offset?: number) =>
  invoke<PlaylistSummary[]>("get_playlists", { limit, offset });
export const getPlaylistTracks = (
  playlistId: string,
  limit?: number,
  offset?: number,
) =>
  invoke<TrackSummary[]>("get_playlist_tracks", { playlistId, limit, offset });
export const getSavedTracks = (limit?: number, offset?: number) =>
  invoke<TrackSummary[]>("get_saved_tracks", { limit, offset });
export const getSavedAlbums = (limit?: number, offset?: number) =>
  invoke<AlbumSummary[]>("get_saved_albums", { limit, offset });
export const getAlbumTracks = (albumId: string) =>
  invoke<TrackSummary[]>("get_album_tracks", { albumId });
/**
 * Followed artists page by cursor, not offset: pass the `next` returned by the
 * previous page. `next: null` means there are no more.
 */
export const getFollowedArtists = (limit?: number, after?: string) =>
  invoke<ArtistPage>("get_followed_artists", { limit, after });
export const getFollowedReleases = (after?: string) =>
  invoke<FollowedReleasePage>("get_followed_releases", { after });
export const getRecentlyPlayed = (limit?: number) =>
  invoke<RecentActivityItem[]>("get_recently_played", { limit });
export const getQuickAccess = (recent: RecentActivityItem[], limit = 6) =>
  invoke<RecentActivityItem[]>("get_quick_access", { recent, limit });
export const recordRelevance = (item: RecentActivityItem, played: boolean) =>
  invoke<void>("record_relevance", { item, played });
export const setTracksSaved = (ids: string[], saved: boolean) =>
  invoke<void>("set_tracks_saved", { ids, saved });
export const setAlbumsSaved = (ids: string[], saved: boolean) =>
  invoke<void>("set_albums_saved", { ids, saved });
export const setArtistsSaved = (ids: string[], saved: boolean) =>
  invoke<void>("set_artists_saved", { ids, saved });
/** Returns one bool per id, in the order given. */
export const getTracksSaved = (ids: string[]) =>
  invoke<boolean[]>("get_tracks_saved", { ids });
export const getAlbumsSaved = (ids: string[]) =>
  invoke<boolean[]>("get_albums_saved", { ids });
export const getArtistsSaved = (ids: string[]) =>
  invoke<boolean[]>("get_artists_saved", { ids });
export const getLikedTracksByArtist = (artistId: string) =>
  invoke<TrackSummary[]>("get_liked_tracks_by_artist", { artistId });
export const getArtistTopTracks = (artistId: string) =>
  invoke<TrackSummary[]>("get_artist_top_tracks", { artistId });
export const getArtistAlbums = (artistId: string, limit = 10, offset = 0) =>
  invoke<AlbumPage>("get_artist_albums", { artistId, limit, offset });
export const getArtist = (artistId: string) =>
  invoke<ArtistSummary>("get_artist", { artistId });
export const getTopTracks = (limit?: number) =>
  invoke<TrackSummary[]>("get_top_tracks", { limit });
export const getTopArtists = (limit?: number) =>
  invoke<ArtistSummary[]>("get_top_artists", { limit });
export const getPersonalizedHome = (limit = 10) =>
  invoke<HomeFeed>("get_personalized_home", {
    limit,
    timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
  });
export const getDjStatus = (refresh = false) =>
  invoke<DjSession | null>("get_dj_status", { refresh });
export const startDj = () => invoke<DjSession>("start_dj");
export const getLyrics = (
  trackUri: string,
  trackName: string,
  artistName: string,
  albumName: string,
  durationMs: number,
) =>
  invoke<LyricsResult>("get_lyrics", {
    trackUri,
    trackName,
    artistName,
    albumName,
    durationMs,
  });

// ---- search -------------------------------------------------------------
export const searchSpotify = (query: string, limit?: number, offset?: number) =>
  invoke<SearchResults>("search_spotify", { query, limit, offset });

// ---- queue --------------------------------------------------------------
export const getQueue = () => invoke<QueueView>("get_queue");
export const addToQueue = (uri: string) => invoke<void>("add_to_queue", { uri });

// ---- jams (experimental) -------------------------------------------------
export const getJamStatus = () => invoke<JamStatus>("get_jam_status");
export const createJam = () => invoke<JamSession>("create_jam");
export const refreshJam = () => invoke<JamSession>("refresh_jam");
export const joinJam = (jamId: string) =>
  invoke<JamSession>("join_jam", { jamId });
export const leaveJam = () => invoke<void>("leave_jam");
/** Adds whatever is currently playing to the active jam. */
export const addTrackToJam = () => invoke<void>("add_track_to_jam");
export const setJamQueueControl = (allowed: boolean) =>
  invoke<JamSession>("set_jam_queue_control", { allowed });
export const kickJamMember = (memberId: string) =>
  invoke<JamSession>("kick_jam_member", { memberId });
export const endJam = () => invoke<void>("end_jam");
