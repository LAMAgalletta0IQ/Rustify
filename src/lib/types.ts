// Mirrors the serde `camelCase` shapes in src-tauri/src/{state,library,search,
// connect,queue}.rs. Keep in sync when the Rust types change.

import type { StreamQuality } from "./generated/StreamQuality";

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
    | "PersistedQueryExpired"
    | "LexiconUnavailable";
  message: string;
  /** Seconds to wait, from Spotify's `Retry-After`. Only set for RateLimited. */
  retryAfter?: number | null;
}

// TrackInfo, PlaybackState, ConnectionStatus, AuthState, Device and
// StreamQuality are generated from their #[derive(TS)] Rust definitions (see
// src-tauri/src/state.rs, connect.rs, audio/mod.rs and CLAUDE.md's ts-rs
// section) rather than hand-mirrored here. Run `npm run gen:types` after
// changing one of those Rust structs; `npm run check:types` fails if the
// committed output has drifted from what the Rust side would generate.
export type { TrackInfo } from "./generated/TrackInfo";
export type { PlaybackState } from "./generated/PlaybackState";
export type { ConnectionStatus } from "./generated/ConnectionStatus";
export type { AuthState } from "./generated/AuthState";

/** Shape of the login flow, from `get_login_info`. */
export interface LoginInfo {
  /** True when a private Web API client ID is set: login opens two browser tabs. */
  privateClientId: boolean;
  /** Env var name carrying the client ID, e.g. `RUSTIFY_CLIENT_ID`. */
  clientIdEnv: string;
  /** Redirect URI to register against a self-registered Spotify app. */
  webapiRedirectUri: string;
  /** The configured ID itself. Not a secret — it is public in every OAuth
   * redirect and this flow is PKCE with no client secret. */
  clientId: string | null;
  /** True when `clientIdEnv` supplied it. The env var takes priority over
   * `settings.json`, so the saved value cannot be edited into effect while
   * this is set. */
  clientIdFromEnv: boolean;
}

export interface DeviceAuthorization {
  userCode: string;
  verificationUri: string;
  verificationUriComplete: string | null;
  url: string;
  expiresIn: number;
  expiresAtMs: number;
  interval: number;
}

export interface SleepTimerStatus {
  active: boolean;
  mode: "duration" | "endOfTrack" | null;
  endsAtUnixMs: number | null;
  remainingSeconds: number | null;
}

export interface AppSettings {
  reduceMotion: boolean;
  cacheLimitMb: number;
  audioQuality: StreamQuality;
  crossfadeSeconds: number;
  outputDevice: string | null;
  equalizer: EqualizerSettings;
  loudness: LoudnessSettings;
  friendsPanelOpen: boolean;
}

export type { StreamQuality } from "./generated/StreamQuality";

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

/** Toward Spotify's own -14 LUFS target, applied by the pinned librespot
 * fork itself (see `player::playback_config` on the Rust side). Takes effect
 * on the next login, not instantly — the same as `crossfadeSeconds`. */
export interface LoudnessSettings {
  enabled: boolean;
  pregainDb: number;
}

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
  isSelected: boolean;
  isActive: boolean;
  isAvailable: boolean;
}

export interface AudioStatus {
  activeDevice: string | null;
  lastError: string | null;
}

export type { Device } from "./generated/Device";

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
  /** Display name of the owner — what the UI shows. */
  owner: string;
  /** Spotify user id of the owner. Compare against `auth.userId` to decide
   * editability; display names are not unique. */
  ownerId: string | null;
  description: string | null;
  collaborative: boolean;
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
  personalization: HomePersonalization | null;
  ownerName: string | null;
  madeForUsername: string | null;
  totalCount: number | null;
  attributes: Record<string, string>;
}

export type HomePersonalization =
  | "dailyMix"
  | "discoverWeekly"
  | "releaseRadar"
  | "daylist"
  | "artistMix"
  | "topicMix"
  | "inspiredByMix"
  | "madeForYou";

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

export interface DjTrack {
  uri: string;
  canonicalUri: string | null;
  source: string | null;
  stationUri: string | null;
  narrationKinds: string[];
  narrationImageUrl: string | null;
}

export interface DjSession {
  available: boolean;
  contextUri: string;
  contextUrl: string | null;
  tracks: DjTrack[];
  metadata: Record<string, string>;
  interactivityEnabled: boolean;
  jumpButtonLabel: string | null;
  volatileContextId: string | null;
  lexiconCurrentTime: string | null;
  lexiconExpirationTime: string | null;
  reason: string;
  active: boolean;
  narrationResolved: boolean;
  narrationPlaybackSupported: boolean;
  dynamicRefillSupported: boolean;
}

/** One axis of the Listening DNA radar, 0-100. See `src-tauri/src/dna.rs` for
 * why this is not Spotify's own DNA and what each axis actually measures. */
export interface DnaAxis {
  id: string;
  label: string;
  value: number;
  basis: string;
}

export interface DnaGenre {
  name: string;
  weight: number;
}

export interface ListeningDna {
  axes: DnaAxis[];
  genres: DnaGenre[];
  artistSample: number;
  trackSample: number;
  /** Too little listening history for the shape to mean anything. */
  sparse: boolean;
  /** Where the genre tags came from. Reports what was actually *used*: a
   * configured Last.fm key that returned nothing still reads `"spotify"`. */
  tagSource: "spotify" | "spotify+lastfm";
  /** How many sampled artists received Last.fm tags. */
  lastfmArtists: number;
}

/** Optional Last.fm enrichment for Listening DNA. Absent is the normal case. */
export interface LastfmConfig {
  configured: boolean;
  apiKey: string | null;
}

export interface LyricsLine {
  startMs: number;
  endMs: number | null;
  text: string;
}

export interface LyricsColors {
  background: number;
  text: number;
  highlightText: number;
}

export interface LyricsResult {
  provider: string;
  status: "available" | "instrumental" | "unavailable";
  syncType: "lineSynced" | "unsynced" | null;
  language: string | null;
  isRtl: boolean;
  colors: LyricsColors | null;
  plain: string | null;
  synced: LyricsLine[];
}

export interface FriendActivity {
  timestampMs: number;
  userUri: string;
  userName: string;
  userImageUrl: string | null;
  trackUri: string;
  trackName: string;
  trackImageUrl: string | null;
  artistUri: string | null;
  artistName: string | null;
  albumUri: string | null;
  albumName: string | null;
  contextUri: string | null;
  contextName: string | null;
}

export interface FriendFeed {
  /** "unavailable" means Spotify actually refused it (403/404) for this
   * account/region. "failed" is any other, expected-to-recover failure
   * (expired token, rate limit, dropped connection, unexpected response) —
   * never worded as a capability limit. */
  status: "connecting" | "available" | "empty" | "stale" | "unavailable" | "failed";
  available: boolean;
  entries: FriendActivity[];
  updatedAtMs: number | null;
}

export interface ProfileArtist {
  uri: string;
  name: string;
  imageUrl: string | null;
  followersCount: number | null;
  isFollowing: boolean | null;
}

export interface ProfilePlaylist {
  uri: string;
  name: string;
  imageUrl: string | null;
  ownerName: string | null;
  ownerUri: string | null;
  isFollowing: boolean | null;
}

export interface UserProfile {
  username: string;
  uri: string;
  displayName: string;
  imageUrl: string | null;
  followingCount: number | null;
  totalPublicPlaylistsCount: number | null;
  isCurrentUser: boolean;
  allowFollows: boolean;
  showFollows: boolean;
  color: number | null;
  recentlyPlayedArtists: ProfileArtist[];
  publicPlaylists: ProfilePlaylist[];
  followers: ProfileArtist[];
  following: ProfileArtist[];
  followersAvailable: boolean;
  followingAvailable: boolean;
}

export interface ConcertEvent {
  uri: string;
  title: string;
  startDateIso: string | null;
  venue: string | null;
  city: string | null;
  isFestival: boolean;
  eventUrl: string | null;
}

export interface ConcertFeed {
  available: boolean;
  totalCount: number;
  events: ConcertEvent[];
}

export interface ArtistStats {
  monthlyListeners: number | null;
  followers: number | null;
}

export interface ArtistOverview {
  stats: ArtistStats;
  topTracks: TrackSummary[];
  concerts: ConcertFeed;
}

export interface CreditContributor {
  name: string;
  artistUri: string | null;
  roles: string[];
}

export interface CreditGroup {
  roleName: string;
  contributors: CreditContributor[];
}

export interface TrackCredits {
  trackName: string | null;
  recordLabel: string | null;
  groups: CreditGroup[];
}

export interface EpisodeResume {
  uri: string;
  positionMs: number;
  completed: boolean;
  hasState: boolean;
}

export interface PlaybackAudit {
  uri: string;
  startedAtMs: number;
  endedAtMs: number;
  playedMs: number;
  finalPositionMs: number;
  completed: boolean;
  endReason: string;
}

export interface TelemetryStatus {
  deliveryAvailable: boolean;
  deliveryTransport: string | null;
  deliveryBlocker: string;
  activePlaybacks: number;
  locallyRecorded: number;
  recent: PlaybackAudit[];
}

export interface MusicVideoImage {
  url: string;
  width: number | null;
  height: number | null;
  size: string;
}

export interface MusicVideoCapability {
  audioUri: string;
  available: boolean;
  videoUri: string | null;
  manifestId: string | null;
  images: MusicVideoImage[];
  openUrl: string | null;
  playbackSupported: boolean;
  playbackBlocker: string | null;
}

export interface AudioFormatCapability {
  id: number;
  name: string;
  codec: string;
  nominalKbps: number | null;
  lossless: boolean;
  bitDepth: number | null;
  decoderSupported: boolean;
  currentPlayerSelectable: boolean;
}

export interface StorageCapability {
  endpointVersion: string;
  attempted: boolean;
  available: boolean;
  result: string | null;
  cdnUrlCount: number;
  error: string | null;
}

export interface AudioCapability {
  trackUri: string;
  formats: AudioFormatCapability[];
  preferredFormat: AudioFormatCapability | null;
  storage: StorageCapability;
  losslessMetadataAvailable: boolean;
  losslessDecoderAvailable: boolean;
  losslessPlaybackAvailable: boolean;
  losslessBlocker: string | null;
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

export interface UserSearchHit {
  uri: string;
  username: string;
  displayName: string;
  imageUrl: string | null;
}

export interface UserSearchPage {
  users: UserSearchHit[];
  total: number;
  nextOffset: number | null;
}

export interface QueueView {
  currentlyPlaying: TrackSummary | null;
  previous: TrackSummary[];
  queue: TrackSummary[];
  autoplay: TrackSummary[];
  revision: string | null;
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
