<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import type { DeviceAuthorization, LoginInfo } from "../types";

  import { onDestroy, onMount } from "svelte";

  function useDifferentClientId() {
    stopCooldown();
    store.setupNeeded = true;
  }

  let busy = $state(false);
  let errKind = $state<string | null>(null);
  let errMsg = $state<string | null>(null);
  let loginInfo = $state<LoginInfo | null>(null);
  let pairing = $state<DeviceAuthorization | null>(null);
  let pairingBusy = $state(false);
  let pairingSeconds = $state(0);
  let pairingTimer: number | null = null;
  let pairRun = 0;

  function stopPairingTimer() {
    if (pairingTimer !== null) {
      clearInterval(pairingTimer);
      pairingTimer = null;
    }
  }

  /** Seconds left on a rate-limit window; null when not waiting. */
  let cooldown = $state<number | null>(null);
  let timer: number | null = null;

  function stopCooldown() {
    if (timer !== null) {
      clearInterval(timer);
      timer = null;
    }
    cooldown = null;
  }

  /**
   * Spotify tells us exactly how long to wait, so sit out the window and retry
   * automatically instead of dead-ending the user on an error they cannot act
   * on. A visible countdown is used rather than a silent await, because these
   * windows run to a minute or more.
   */
  function startCooldown(secs: number) {
    stopCooldown();
    cooldown = secs;
    timer = window.setInterval(() => {
      if (cooldown === null) return;
      cooldown -= 1;
      if (cooldown <= 0) {
        stopCooldown();
        // Silent retry: the OAuth flow already succeeded and its refresh token
        // is stored, so this must not reopen the browser.
        doLogin(true);
      }
    }, 1000);
  }

  async function doLogin(silent = false) {
    stopCooldown();
    busy = true;
    errKind = null;
    errMsg = null;
    try {
      // `restoreSession` reuses the stored refresh token and never opens a
      // browser. It returns a logged-out state if nothing is stored, in which
      // case fall through to the full interactive flow.
      if (silent) {
        const restored = await api.restoreSession();
        if (restored.loggedIn) {
          store.auth = restored;
          return;
        }
      }
      store.auth = await api.login();
    } catch (e) {
      const err = api.asAppError(e);
      errKind = err.kind;
      errMsg = err.message;
      // +2s of slack: retrying the instant the window expires tends to be
      // refused again.
      if (err.kind === "RateLimited" && err.retryAfter) {
        startCooldown(err.retryAfter + 2);
      }
    } finally {
      busy = false;
    }
  }

  async function startPairing() {
    const run = ++pairRun;
    busy = true;
    errKind = null;
    errMsg = null;
    try {
      pairing = await api.startDeviceAuthorization();
      pairingSeconds = Math.max(0, Math.ceil((pairing.expiresAtMs - Date.now()) / 1000));
      if (pairingTimer !== null) clearInterval(pairingTimer);
      pairingTimer = window.setInterval(() => {
        if (!pairing) return;
        pairingSeconds = Math.max(0, Math.ceil((pairing.expiresAtMs - Date.now()) / 1000));
      }, 1000);
      try {
        await openUrl(pairing.url);
      } catch {
        // The code and URL stay visible for headless/manual completion.
      }
      busy = false;
      pairingBusy = true;
      const authorized = await api.completeDeviceAuthorization();
      if (run === pairRun) {
        store.auth = authorized;
        pairing = null;
        stopPairingTimer();
      }
    } catch (e) {
      if (run === pairRun) {
        const err = api.asAppError(e);
        errKind = err.kind;
        errMsg = err.message;
        pairing = null;
        stopPairingTimer();
      }
    } finally {
      if (run === pairRun) {
        busy = false;
        pairingBusy = false;
      }
    }
  }

  async function cancelPairing() {
    pairRun += 1;
    pairing = null;
    pairingBusy = false;
    stopPairingTimer();
    await api.cancelDeviceAuthorization().catch(() => {});
  }

  onMount(async () => {
    loginInfo = await api.getLoginInfo().catch(() => null);
  });
  onDestroy(() => {
    stopCooldown();
    stopPairingTimer();
    if (pairing) void api.cancelDeviceAuthorization();
  });
</script>

<div class="wrap">
  <div class="card">
    <h1>Rustify</h1>
    <p class="muted">
      A native, lightweight Spotify client. Playback is handled by librespot;
      library and search use the official Web API.
    </p>

    <button
      class="btn-primary"
      onclick={() => doLogin()}
      disabled={busy || pairingBusy || cooldown !== null || loginInfo?.privateClientId === false}
    >
      {busy
        ? "Waiting for browser…"
        : cooldown !== null
          ? `Retrying in ${cooldown}s…`
          : "Log in with Spotify"}
    </button>

    {#if loginInfo?.privateClientId === false}
      <p class="muted small">
        Browser login needs a dashboard Client ID. You can configure one below,
        or use device pairing now.
      </p>
    {/if}

    <button class="secondary" onclick={startPairing} disabled={busy || pairingBusy}>
      {pairingBusy ? "Waiting for approval…" : "Pair with a code"}
    </button>

    {#if pairing}
      <div class="pairing">
        <span class="muted small">Enter this code at <strong>{pairing.verificationUri}</strong> · expires in {pairingSeconds}s</span>
        <code>{pairing.userCode}</code>
        <div class="pair-actions">
          <button class="secondary" onclick={() => openUrl(pairing!.url)}>Open pairing page</button>
          <button class="linklike small" onclick={cancelPairing}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if busy && !pairing}
      <p class="muted small">
        Your browser has been opened. Approve access — then approve a
        <strong>second</strong> time, in the tab that opens after it. One
        login is for playback, the other for library and search.
      </p>
    {/if}

    {#if errMsg}
      <div
        class="err"
        class:premium={errKind === "PremiumRequired" ||
          errKind === "RateLimited"}
      >
        <strong>
          {errKind === "PremiumRequired"
            ? "Premium required"
            : errKind === "RateLimited"
              ? "Rate limited by Spotify"
              : "Login failed"}
        </strong>
        <span>{errMsg}</span>
        {#if errKind === "RateLimited"}
          <span class="small">
            This is most likely a short burst against your app's own quota,
            not the shared quota problem unregistered apps run into.
            {#if cooldown !== null}
              Waiting out Spotify's window and retrying automatically.
            {:else}
              Waiting a few minutes and trying again normally clears it.
            {/if}
          </span>
          <span class="small">
            If it keeps happening, the Client ID may be wrong or the app may
            have been deleted from the dashboard.
          </span>
        {/if}
        {#if errKind === "PremiumRequired"}
          <span class="small">
            librespot can only stream for Premium accounts — the free,
            ad-supported tier is not supported. This is a limitation of the
            playback library, not something the app can work around.
          </span>
        {/if}
      </div>
    {/if}

    <button class="linklike small" onclick={useDifferentClientId}>
      Use a different Client ID
    </button>
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex: 1;
    min-height: 0;
    width: 100%;
    padding: 24px;
    overflow-y: auto;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 16px;
    align-items: flex-start;
    max-width: 420px;
    padding: 32px;
    background: var(--glass);
    border: 1px solid var(--hairline);
    border-radius: var(--r-lg);
    backdrop-filter: blur(var(--blur));
    margin: auto;
  }
  h1 {
    margin: 0;
    font-size: 24px;
  }
  p {
    margin: 0;
    line-height: 1.5;
  }
  .small {
    font-size: 12px;
  }
  .err {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 14px;
    border-radius: var(--r-sm);
    background: rgba(180, 50, 60, 0.22);
    border: 1px solid rgba(220, 90, 100, 0.35);
    line-height: 1.45;
  }
  .err.premium {
    background: rgba(190, 160, 50, 0.18);
    border-color: rgba(220, 190, 90, 0.3);
  }
  .pairing {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--hairline);
    border-radius: var(--r-sm);
    background: var(--glass-strong);
  }
  .pairing code {
    font-size: 28px;
    font-weight: 700;
    letter-spacing: 0.14em;
    color: var(--accent);
  }
  .pair-actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .linklike {
    color: var(--fg-dim);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .linklike:hover {
    color: var(--fg);
  }
</style>
