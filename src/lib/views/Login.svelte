<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";

  import { onDestroy } from "svelte";

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
    <h1>spotify-rust</h1>
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
        Your browser has been opened. Approve access, then return here.
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
            This is usually not caused by anything you did. librespot's built-in
            client ID is shared by every librespot-based app, so its Spotify API
            quota is used up globally — a fresh login can be refused.
            {#if cooldown !== null}
              Waiting out Spotify's window and retrying automatically.
            {:else}
              Waiting a few minutes and trying again normally clears it.
            {/if}
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
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: 12px;
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
    border-radius: var(--radius);
    background: #2a1618;
    border: 1px solid #57282c;
    line-height: 1.45;
  }
  .err.premium {
    background: #2a2416;
    border-color: #574d28;
  }
</style>
