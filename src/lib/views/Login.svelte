<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";

  import { onDestroy, onMount } from "svelte";

  /** Null until loaded; the copy below degrades gracefully while it is. */
  let info = $state<import("../types").LoginInfo | null>(null);

  onMount(async () => {
    try {
      info = await api.getLoginInfo();
    } catch {
      // Purely cosmetic — a failure here must not block logging in.
    }
  });

  let busy = $state(false);
  let errKind = $state<string | null>(null);
  let errMsg = $state<string | null>(null);

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

  onDestroy(stopCooldown);
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
      disabled={busy || cooldown !== null}
    >
      {busy
        ? "Waiting for browser…"
        : cooldown !== null
          ? `Retrying in ${cooldown}s…`
          : "Log in with Spotify"}
    </button>

    {#if busy}
      <p class="muted small">
        {#if info?.privateClientId}
          Your browser has been opened. Approve access — then approve a
          <strong>second</strong> time, in the tab that opens after it. One
          login is for playback, the other for library and search.
        {:else}
          Your browser has been opened. Approve access, then return here.
        {/if}
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
            {#if info?.privateClientId}
              Unusual with your own client ID configured — this quota is
              private to your app, so it is most likely a short burst rather
              than the shared-quota problem.
            {:else}
              This is not caused by anything you did. Without a client ID of
              your own, library and search share librespot's built-in one with
              every librespot-based app in the world, so its Spotify API quota
              is used up globally and a fresh login can be refused.
            {/if}
            {#if cooldown !== null}
              Waiting out Spotify's window and retrying automatically.
            {:else}
              Waiting a few minutes and trying again normally clears it.
            {/if}
          </span>
          {#if info && !info.privateClientId}
            <span class="small">
              To fix it properly: create an app at
              <code>developer.spotify.com/dashboard</code>, add
              <code>{info.webapiRedirectUri}</code> as a redirect URI, and put
              the Client ID in <code>.env</code> as
              <code>{info.clientIdEnv}</code>.
            </span>
          {/if}
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
  </div>
</div>

<style>
  .wrap {
    display: grid;
    place-items: center;
    height: 100%;
    padding: 24px;
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
  code {
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: rgba(255, 241, 224, 0.08);
    word-break: break-all;
  }
</style>
