import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import * as api from "./api";
import { EVENT_AUTH, EVENT_PLAYBACK } from "./api";
import type { AuthState, PlaybackState } from "./types";

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
};

const emptyAuth: AuthState = {
  loggedIn: false,
  displayName: null,
  userId: null,
  product: null,
  avatarUrl: null,
};

class AppStore {
  playback = $state<PlaybackState>({ ...emptyPlayback });
  auth = $state<AuthState>({ ...emptyAuth });
  /** Non-fatal error banner text. */
  error = $state<string | null>(null);
  booting = $state(true);

  #unlisten: UnlistenFn[] = [];
  #ticker: number | null = null;

  async init() {
    this.#unlisten.push(
      await listen<PlaybackState>(EVENT_PLAYBACK, (e) => {
        this.playback = e.payload;
        this.#syncTicker();
      }),
    );
    this.#unlisten.push(
      await listen<AuthState>(EVENT_AUTH, (e) => {
        this.auth = e.payload;
      }),
    );

    try {
      this.auth = await api.restoreSession();
      if (this.auth.loggedIn) {
        this.playback = await api.getPlayback();
        this.#syncTicker();
      }
    } catch (e) {
      // A failed restore is not fatal — fall through to the login screen.
      console.warn("session restore failed", api.asAppError(e).message);
    } finally {
      this.booting = false;
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
      this.error = api.asAppError(e).message;
    }
  }
}

export const store = new AppStore();
