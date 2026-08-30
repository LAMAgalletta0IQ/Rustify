import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, PlaybackState, TrackInfo } from "./types";

type EventPayload<T> = { payload: T };
type EventCallback = (event: EventPayload<unknown>) => void;

const { listeners, invokeMock } = vi.hoisted(() => {
  return {
    listeners: new Map<string, EventCallback>(),
    invokeMock: vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>(),
  };
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((eventName: string, cb: EventCallback) => {
    listeners.set(eventName, cb);
    return Promise.resolve(() => {
      listeners.delete(eventName);
    });
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));

const { AppStore } = await import("./store.svelte");
const { EVENT_AUTH, EVENT_PLAYBACK } = await import("./api");

function track(overrides: Partial<TrackInfo> = {}): TrackInfo {
  return {
    uri: "spotify:track:one",
    name: "Track One",
    artists: ["Artist"],
    album: "Album",
    albumId: "album-1",
    albumUri: "spotify:album:one",
    coverUrl: null,
    durationMs: 200_000,
    ...overrides,
  };
}

function playback(overrides: Partial<PlaybackState> = {}): PlaybackState {
  return {
    isPlaying: false,
    isLoading: false,
    isActiveDevice: true,
    track: track(),
    positionMs: 0,
    durationMs: 200_000,
    volume: 32768,
    shuffle: false,
    repeatContext: false,
    repeatTrack: false,
    contextUri: null,
    activeDevice: null,
    availableDevices: [],
    connectionStatus: "connected",
    audioQuality: "automatic",
    audioQualityLabel: "160 kbps",
    ...overrides,
  };
}

function settings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
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
    friendsPanelOpen: true,
    ...overrides,
  };
}

function firePlayback(overrides: Partial<PlaybackState> = {}) {
  const cb = listeners.get(EVENT_PLAYBACK);
  if (!cb) throw new Error("nothing listens for " + EVENT_PLAYBACK);
  cb({ payload: playback(overrides) });
}

/** Wires up the default set of `init()` calls; individual tests override the
 * commands they care about with `invokeMock.mockImplementation`. */
function mockDefaultInvokes() {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_settings":
        return Promise.resolve(settings());
      case "get_login_info":
        return Promise.resolve({
          privateClientId: true,
          clientIdEnv: "RUSTIFY_CLIENT_ID",
          webapiRedirectUri: "http://127.0.0.1:8899/login",
          clientId: "abc",
          clientIdFromEnv: false,
        });
      case "get_auth_state":
        return Promise.resolve({
          loggedIn: false,
          displayName: null,
          userId: null,
          product: null,
          avatarUrl: null,
        });
      case "restore_session":
        return Promise.resolve({
          loggedIn: false,
          displayName: null,
          userId: null,
          product: null,
          avatarUrl: null,
        });
      default:
        return Promise.reject(new Error(`unhandled invoke in test: ${cmd}`));
    }
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  listeners.clear();
  invokeMock.mockReset();
  mockDefaultInvokes();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("handleError", () => {
  it("clears auth on SessionExpired", () => {
    const store = new AppStore();
    store.auth = {
      loggedIn: true,
      displayName: "Gabo",
      userId: "1",
      product: "premium",
      avatarUrl: null,
    };
    const result = store.handleError({ kind: "SessionExpired", message: "expired" });
    expect(result.kind).toBe("SessionExpired");
    expect(store.auth.loggedIn).toBe(false);
    expect(store.error).toBe("expired");
  });

  it("clears auth on NotLoggedIn", () => {
    const store = new AppStore();
    store.auth = {
      loggedIn: true,
      displayName: "Gabo",
      userId: "1",
      product: "premium",
      avatarUrl: null,
    };
    store.handleError({ kind: "NotLoggedIn", message: "not logged in" });
    expect(store.auth.loggedIn).toBe(false);
  });

  it("leaves auth untouched for an unrelated error kind", () => {
    const store = new AppStore();
    store.auth = {
      loggedIn: true,
      displayName: "Gabo",
      userId: "1",
      product: "premium",
      avatarUrl: null,
    };
    store.handleError({ kind: "BadRequest", message: "bad" });
    expect(store.auth.loggedIn).toBe(true);
  });
});

describe("position ticker", () => {
  it("starts when playing and advances position once per second", async () => {
    const store = new AppStore();
    await store.init();

    firePlayback({ isPlaying: true, positionMs: 0, durationMs: 10_000 });
    expect(store.playback.positionMs).toBe(0);

    await vi.advanceTimersByTimeAsync(1000);
    expect(store.playback.positionMs).toBe(1000);

    await vi.advanceTimersByTimeAsync(2000);
    expect(store.playback.positionMs).toBe(3000);
  });

  it("stops advancing once playback is no longer playing", async () => {
    const store = new AppStore();
    await store.init();

    firePlayback({ isPlaying: true, positionMs: 0, durationMs: 10_000 });
    await vi.advanceTimersByTimeAsync(1000);
    expect(store.playback.positionMs).toBe(1000);

    firePlayback({ isPlaying: false, positionMs: 1000, durationMs: 10_000 });
    await vi.advanceTimersByTimeAsync(3000);
    expect(store.playback.positionMs).toBe(1000);
  });

  it("clamps position at duration instead of overshooting", async () => {
    const store = new AppStore();
    await store.init();

    firePlayback({ isPlaying: true, positionMs: 500, durationMs: 1000 });
    await vi.advanceTimersByTimeAsync(1000);
    expect(store.playback.positionMs).toBe(1000);
  });
});

describe("lyrics request cancellation", () => {
  it("discards a stale lyrics response for a track that is no longer current", async () => {
    const store = new AppStore();

    const deferred = new Map<
      string,
      { promise: Promise<unknown>; resolve: (v: unknown) => void }
    >();
    function deferredFor(uri: string) {
      if (!deferred.has(uri)) {
        let resolve!: (v: unknown) => void;
        const promise = new Promise((r) => {
          resolve = r;
        });
        deferred.set(uri, { promise, resolve });
      }
      return deferred.get(uri)!;
    }

    await store.init();

    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_lyrics") {
        const uri = args?.trackUri as string;
        return deferredFor(uri).promise as Promise<unknown>;
      }
      return Promise.reject(new Error(`unhandled invoke in test: ${cmd}`));
    });

    // Two tracks change in quick succession, before either lyrics fetch
    // resolves — the second (current) request must win even if the first
    // (stale) one resolves after it.
    firePlayback({ track: track({ uri: "spotify:track:first" }) });
    firePlayback({ track: track({ uri: "spotify:track:second" }) });

    deferredFor("spotify:track:second").resolve({
      lines: [],
      source: "lrclib",
    });
    await Promise.resolve();
    await Promise.resolve();

    deferredFor("spotify:track:first").resolve({
      lines: [{ text: "stale line" }],
      source: "spotify",
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(store.lyrics).toEqual({ lines: [], source: "lrclib" });
  });
});

describe("toggleFriendsPanel", () => {
  it("rolls back the optimistic toggle when the backend command fails", async () => {
    const store = new AppStore();
    await store.init();
    expect(store.settings.friendsPanelOpen).toBe(true);

    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "update_settings") {
        return Promise.reject({ kind: "Other", message: "disk full" });
      }
      return Promise.reject(new Error(`unhandled invoke in test: ${cmd}`));
    });

    await store.toggleFriendsPanel();

    expect(store.settings.friendsPanelOpen).toBe(true);
    expect(store.error).toBe("disk full");
  });

  it("keeps the toggle when the backend command succeeds", async () => {
    const store = new AppStore();
    await store.init();

    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "update_settings") {
        return Promise.resolve(args?.settings);
      }
      return Promise.reject(new Error(`unhandled invoke in test: ${cmd}`));
    });

    await store.toggleFriendsPanel();
    expect(store.settings.friendsPanelOpen).toBe(false);
  });
});

describe("destroy", () => {
  it("clears the ticker interval and unregisters event listeners", async () => {
    const store = new AppStore();
    await store.init();
    firePlayback({ isPlaying: true, positionMs: 0, durationMs: 10_000 });
    await vi.advanceTimersByTimeAsync(1000);
    expect(store.playback.positionMs).toBe(1000);

    expect(listeners.has(EVENT_PLAYBACK)).toBe(true);
    expect(listeners.has(EVENT_AUTH)).toBe(true);

    store.destroy();

    expect(listeners.has(EVENT_PLAYBACK)).toBe(false);
    expect(listeners.has(EVENT_AUTH)).toBe(false);

    const positionAfterDestroy = store.playback.positionMs;
    await vi.advanceTimersByTimeAsync(3000);
    expect(store.playback.positionMs).toBe(positionAfterDestroy);
  });
});
