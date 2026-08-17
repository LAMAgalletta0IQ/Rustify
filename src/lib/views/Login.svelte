<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";

  let busy = $state(false);
  let errKind = $state<string | null>(null);
  let errMsg = $state<string | null>(null);

  async function doLogin() {
    busy = true;
    errKind = null;
    errMsg = null;
    try {
      store.auth = await api.login();
    } catch (e) {
      const err = api.asAppError(e);
      errKind = err.kind;
      errMsg = err.message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <div class="card">
    <h1>spotify-rust</h1>
    <p class="muted">
      A native, lightweight Spotify client. Playback is handled by librespot;
      library and search use the official Web API.
    </p>

    <button class="btn-primary" onclick={doLogin} disabled={busy}>
      {busy ? "Waiting for browser…" : "Log in with Spotify"}
    </button>

    {#if busy}
      <p class="muted small">
        Your browser has been opened. Approve access, then return here.
      </p>
    {/if}

    {#if errMsg}
      <div class="err" class:premium={errKind === "PremiumRequired"}>
        <strong>
          {errKind === "PremiumRequired"
            ? "Premium required"
            : "Login failed"}
        </strong>
        <span>{errMsg}</span>
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
