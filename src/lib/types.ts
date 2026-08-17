// Mirrors the serde `camelCase` shapes in src-tauri/src/{state,library,search,
// connect,queue}.rs. Keep in sync when the Rust types change.

export interface AppErrorPayload {
  kind:
    | "NotLoggedIn"
    | "PremiumRequired"
    | "Auth"
    | "Playback"
    | "WebApi"
    | "RateLimited"
    | "Other";
  message: string;
  /** Seconds to wait, from Spotify's `Retry-After`. Only set for RateLimited. */
  retryAfter?: number | null;
}

export interface TrackInfo {
  uri: string;
  name: string;
  artists: string[];
  album: string;
  coverUrl: string | null;
  durationMs: number;
}

export interface PlaybackState {
  isPlaying: boolean;
  isLoading: boolean;
  isActiveDevice: boolean;
  track: TrackInfo | null;
  positionMs: number;
  durationMs: number;
  /** librespot scale: 0..=65535 */
  volume: number;
  shuffle: boolean;
  repeatContext: boolean;
  repeatTrack: boolean;
}

export interface AuthState {
  loggedIn: boolean;
  displayName: string | null;
  userId: string | null;
  product: string | null;
  avatarUrl: string | null;
}

/** Shape of the login flow, from `get_login_info`. */
export interface LoginInfo {
  /** True when a private Web API client ID is set: login opens two browser tabs. */
  privateClientId: boolean;
  /** Env var name carrying the client ID, e.g. `RUSTIFY_CLIENT_ID`. */
  clientIdEnv: string;
  /** Redirect URI to register against a self-registered Spotify app. */
  webapiRedirectUri: string;
}

export interface Device {
  id: string | null;
  name: string;
  type: string;
  isActive: boolean;
  isRestricted: boolean;
  volumePercent: number | null;
}

export interface TrackSummary {
  uri: string;
  id: string;
  name: string;
  artists: string[];
  album: string;
  imageUrl: string | null;
  durationMs: number;
}

export interface PlaylistSummary {
  uri: string;
  id: string;
  name: string;
  owner: string;
  imageUrl: string | null;
  trackCount: number;
}

export interface AlbumSummary {
  uri: string;
  id: string;
  name: string;
  artists: string[];
  imageUrl: string | null;
}

export interface ArtistSummary {
  uri: string;
  id: string;
  name: string;
  imageUrl: string | null;
}

/** Cursor-paginated: `next` feeds the `after` argument of the following call. */
export interface ArtistPage {
  items: ArtistSummary[];
  next: string | null;
}

export interface PlaylistHit {
  uri: string;
  id: string;
  name: string;
  owner: string;
  imageUrl: string | null;
}

export interface SearchResults {
  tracks: TrackSummary[];
  albums: AlbumSummary[];
  artists: ArtistSummary[];
  playlists: PlaylistHit[];
}

export interface QueueView {
  currentlyPlaying: TrackSummary | null;
  queue: TrackSummary[];
}

export const MAX_VOLUME = 65535;

export function volumeToPercent(v: number): number {
  return Math.round((v * 100) / MAX_VOLUME);
}

export function formatMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0:00";
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
