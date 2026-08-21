// Mirrors the serde `camelCase` shapes in src-tauri/src/{state,library,search,
// connect,queue}.rs. Keep in sync when the Rust types change.

export interface AppErrorPayload {
  kind:
    | "NotLoggedIn"
    | "PremiumRequired"
    | "Auth"
    | "Playback"
    | "WebApi"
    | "SessionExpired"
    | "BadRequest"
    | "Forbidden"
    | "Unavailable"
    | "ServiceUnavailable"
    | "RateLimited"
    | "Other"
    | "FeatureUnsupported"
    | "PublicApiLimitation"
    | "EndpointNotAvailable"
    | "PersistedQueryExpired";
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
  audioQuality: StreamQuality;
  audioQualityLabel: string;
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

export interface AppSettings {
  defaultVolumePercent: number;
  reduceMotion: boolean;
  cacheLimitMb: number;
  audioQuality: StreamQuality;
  outputDevice: string | null;
  equalizer: EqualizerSettings;
}

export type StreamQuality = "automatic" | "low" | "normal" | "veryHigh";

export interface EqualizerPreset {
  id: string;
  name: string;
  bandsDb: [number, number, number, number, number, number];
  preampDb: number;
}

export interface EqualizerSettings {
  enabled: boolean;
  bandsDb: [number, number, number, number, number, number];
  preampDb: number;
  autoHeadroom: boolean;
  activePresetId: string | null;
  customPresets: EqualizerPreset[];
}

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
  isSelected: boolean;
  isActive: boolean;
}

export interface AudioStatus {
  activeDevice: string | null;
  lastError: string | null;
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
  artistIds: string[];
  album: string;
  imageUrl: string | null;
  durationMs: number;
  explicit: boolean;
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
  albumType?: string;
  releaseDate?: string | null;
  releaseDatePrecision?: string | null;
  totalTracks?: number | null;
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

export interface AlbumPage {
  items: AlbumSummary[];
  hasMore: boolean;
}

export interface FollowedReleasePage {
  items: AlbumSummary[];
  nextArtist: string | null;
  partialErrors: string[];
}

export type RecentActivityKind = "track" | "album" | "playlist" | "artist";

export interface RecentActivityItem {
  kind: RecentActivityKind;
  uri: string;
  id: string;
  name: string;
  subtitle: string;
  imageUrl: string | null;
  lastPlayedAt: string;
  trackUri: string | null;
  frequency: number;
}

export interface HomeItem {
  kind: string;
  uri: string;
  id: string;
  name: string;
  subtitle: string | null;
  description: string | null;
  imageUrl: string | null;
  typeName: string;
  format: string | null;
  ownerName: string | null;
  madeForUsername: string | null;
  totalCount: number | null;
  attributes: Record<string, string>;
}

export interface HomeSection {
  uri: string;
  title: string | null;
  typeName: string;
  totalCount: number;
  items: HomeItem[];
}

export interface HomeFeed {
  greeting: string | null;
  sections: HomeSection[];
}

export interface LyricsLine {
  startMs: number;
  text: string;
}

export interface LyricsResult {
  provider: "LRCLIB";
  status: "available" | "instrumental" | "unavailable";
  plain: string | null;
  synced: LyricsLine[];
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
  hasMore: boolean;
}

export interface QueueView {
  currentlyPlaying: TrackSummary | null;
  queue: TrackSummary[];
}

// ---- jams (experimental) ---------------------------------------------------
// Mirrors src-tauri/src/jams/session.rs and dealer.rs.

export interface JamMember {
  id: string;
  name: string | null;
  isHost: boolean;
  username: string | null;
  displayName: string | null;
  imageUrl: string | null;
  largeImageUrl: string | null;
  joinedTimestamp: number | null;
  isListening: boolean;
  isControlling: boolean;
  playbackControl: string | null;
  isCurrentUser: boolean;
}

export interface JamTrack {
  uri: string;
  name: string | null;
  artists: string[];
  addedBy: string | null;
  addedAt: string | null;
}

export interface JamSession {
  id: string;
  timestamp: number | null;
  ownerId: string | null;
  host: JamMember | null;
  members: JamMember[];
  queue: JamTrack[];
  activeState: unknown;
  /** Token from the invite link — what `joinJam` takes. */
  joinToken: string | null;
  /** Shareable `https://open.spotify.com/socialsession/<token>` link. */
  joinUrl: string | null;
  joinUri: string | null;
  isSessionOwner: boolean;
  isListening: boolean;
  isControlling: boolean;
  isDiscoverable: boolean;
  sessionType: string | null;
  hostActiveDeviceId: string | null;
  maxMemberCount: number | null;
  active: boolean;
  queueOnlyMode: boolean;
  queueControlAllowed: boolean;
  wifiBroadcast: boolean;
  hostDeviceInfo: unknown | null;
}

/** Status of the jam backend, from `get_jam_status`. */
export interface JamStatus {
  spclientEndpoints: number;
  pathfinderHashes: number;
  session: JamSession | null;
}

/**
 * Dealer events forwarded as `jams:changed`. Variant/field names are the
 * serialised `camelCase` form of the Rust `JamEvent` enum; kept loose because
 * the payload shapes are still being captured.
 */
export type JamEventPayload = Record<string, unknown>;

export const MAX_VOLUME = 65535;

export function volumeToPercent(v: number): number {
  return Math.round((v * 100) / MAX_VOLUME);
}

/** "1 track" / "2 tracks" — playlists of one are common enough to notice. */
export function trackCountLabel(n: number): string {
  return `${n} ${n === 1 ? "track" : "tracks"}`;
}

export function formatMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0:00";
  const total = Math.floor(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}
