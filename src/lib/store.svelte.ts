import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import * as api from "./api";
import { EVENT_AUTH, EVENT_PLAYBACK } from "./api";
import type {
  AppErrorPayload,
  AppSettings,
  AuthState,
  LoginInfo,
  LyricsResult,
  PlaybackState,
} from "./types";

const emptyPlayback: PlaybackState = {
  isPlaying: false,
  isLoading: false,
  isActiveDevice: false,
  track: null,
  positionMs: 0,
  durationMs: 0,
  volume: 32768,
  shuffle: false,
  repeatContext: false,
  repeatTrack: false,
  contextUri: null,
  activeDevice: null,
  availableDevices: [],
  connectionStatus: "disconnected",
  audioQuality: "automatic",
  audioQualityLabel: "160 kbps",
};

const emptyAuth: AuthState = {
  loggedIn: false,
  displayName: null,
  userId: null,
  product: null,
  avatarUrl: null,
};

const defaultSettings: AppSettings = {
  reduceMotion: false,
  cacheLimitMb: 2048,
  audioQuality: "automatic",
  crossfadeSeconds: 0,
  outputDevice: null,
  equalizer: {
    enabled: false,
    bandsDb: [0, 0, 0, 0, 0, 0],
    preampDb: 0,
    autoHeadroom: true,
    activePresetId: "flat",
    customPresets: [],
  },
};

class AppStore {
  playback = $state<PlaybackState>({ ...emptyPlayback });
  auth = $state<AuthState>({ ...emptyAuth });
  /** Non-fatal error banner text. */
  error = $state<string | null>(null);
  booting = $state(true);
  /** True until a Web API client ID is configured; gates Login behind Setup. */
  setupNeeded = $state(false);
  settings = $state<AppSettings>({ ...defaultSettings });
  lyrics = $state<LyricsResult | null>(null);
  lyricsLoading = $state(false);
  lyricsError = $state<string | null>(null);

  #unlisten: UnlistenFn[] = [];
  #ticker: number | null = null;
  #intentionalLogout = false;
  #lyricsRequest = 0;

  async init() {
    this.#unlisten.push(
      await listen<PlaybackState>(EVENT_PLAYBACK, (e) => {
        const previousTrack = this.playback.track?.uri;
        this.playback = e.payload;
        if (previousTrack !== e.payload.track?.uri) {
          this.#syncLyrics(e.payload.track);
        }
        this.#syncTicker();
      }),
    );
    this.#unlisten.push(
      await listen<AuthState>(EVENT_AUTH, (e) => {
        const expired = this.auth.loggedIn && !e.payload.loggedIn;
        this.auth = e.payload;
        if (expired && !this.#intentionalLogout) {
          this.error = "Your Spotify session expired. Sign in again to continue.";
        }
      }),
    );

    try {
      try {
        this.settings = await api.getSettings();
      } catch (e) {
        console.warn("could not read settings", api.asAppError(e).message);
      }
      let info: LoginInfo | null = null;
      try {
        info = await api.getLoginInfo();
      } catch (e) {
        // Purely cosmetic below — a failure here must not block login.
        console.warn("could not read login info", api.asAppError(e).message);
      }
      // A webview reload (including development HMR) does not restart Rust.
      // Reuse that live session instead of consuming refresh tokens again.
      const liveAuth = await api.getAuthState();
      this.auth = liveAuth.loggedIn ? liveAuth : await api.restoreSession();
      if (this.auth.loggedIn) {
        this.playback = await api.getPlayback();
        this.#syncLyrics(this.playback.track);
        this.#syncTicker();
      } else if (info && !info.privateClientId) {
        // A private client ID is required only for the two-tab loopback flow.
        // Setup also offers device pairing, which uses the streaming client's
        // accepted RFC 8628 capability and therefore needs no dashboard app.
        this.setupNeeded = true;
      }
    } catch (e) {
      // A failed restore is not fatal — fall through to the login screen.
      console.warn("session restore failed", api.asAppError(e).message);
    } finally {
      this.booting = false;
    }
  }

  async saveSettings(settings: AppSettings) {
    this.settings = await api.updateSettings(settings);
  }

  async saveAudioSettings(
    outputDevice: string | null,
    equalizer: AppSettings["equalizer"],
  ) {
    this.settings = await api.updateAudioSettings(outputDevice, equalizer);
  }

  #syncLyrics(track: PlaybackState["track"]) {
    const request = ++this.#lyricsRequest;
    this.lyrics = null;
    this.lyricsError = null;
    this.lyricsLoading = Boolean(track);
    if (!track) return;
    void api
      .getLyrics(
        track.uri,
        track.name,
        track.artists[0] ?? "",
        track.album,
        track.durationMs,
      )
      .then((lyrics) => {
        if (request === this.#lyricsRequest) this.lyrics = lyrics;
      })
      .catch((error) => {
        if (request === this.#lyricsRequest) {
          this.lyricsError = api.asAppError(error).message;
        }
      })
      .finally(() => {
        if (request === this.#lyricsRequest) this.lyricsLoading = false;
      });
  }

  async logout() {
    this.#intentionalLogout = true;
    try {
      await api.logout();
      this.auth = { ...emptyAuth };
      this.playback = { ...emptyPlayback };
      this.#syncLyrics(null);
    } finally {
      this.#intentionalLogout = false;
    }
  }

  /** Called once Setup has saved a client ID, to move on to the Login screen. */
  async finishSetup() {
    try {
      const info = await api.getLoginInfo();
      this.setupNeeded = !info.privateClientId;
    } catch (e) {
      console.warn("could not confirm setup", api.asAppError(e).message);
      this.setupNeeded = false;
    }
  }

  /**
   * The backend only emits position updates periodically. Tick locally at 1Hz
   * while playing so the progress bar moves without burning CPU on a
   * requestAnimationFrame loop.
   */
  #syncTicker() {
    const shouldRun = this.playback.isPlaying;
    if (shouldRun && this.#ticker === null) {
      this.#ticker = window.setInterval(() => {
        if (!this.playback.isPlaying) return;
        const next = this.playback.positionMs + 1000;
        this.playback.positionMs = Math.min(
          next,
          this.playback.durationMs || next,
        );
      }, 1000);
    } else if (!shouldRun && this.#ticker !== null) {
      clearInterval(this.#ticker);
      this.#ticker = null;
    }
  }

  destroy() {
    this.#lyricsRequest += 1;
    this.#unlisten.forEach((f) => f());
    this.#unlisten = [];
    if (this.#ticker !== null) clearInterval(this.#ticker);
  }

  /** Wraps a backend call, surfacing failures in the error banner. */
  async run(fn: () => Promise<unknown>) {
    try {
      this.error = null;
      await fn();
    } catch (e) {
      this.handleError(e);
    }
  }

  /** Normalises a command failure and consistently expires invalid sessions. */
  handleError(e: unknown, showBanner = true): AppErrorPayload {
    const error = api.asAppError(e);
    if (showBanner) this.error = error.message;
    if (error.kind === "SessionExpired" || error.kind === "NotLoggedIn") {
      this.auth = { ...emptyAuth };
    }
    return error;
  }
}

export const store = new AppStore();
