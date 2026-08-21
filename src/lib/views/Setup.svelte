<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "../api";

  let { onDone, onPair }: { onDone: () => void; onPair: () => void } = $props();

  /** Null until loaded; the redirect URI below degrades gracefully while it is. */
  let info = $state<import("../types").LoginInfo | null>(null);

  onMount(async () => {
    try {
      info = await api.getLoginInfo();
    } catch {
      // Purely cosmetic — a failure here must not block filling in the field.
    }
  });

  let clientId = $state("");
  let busy = $state(false);
  let errMsg = $state<string | null>(null);

  async function save() {
    const trimmed = clientId.trim();
    if (!trimmed) return;
    busy = true;
    errMsg = null;
    try {
      await api.setClientId(trimmed);
      onDone();
    } catch (e) {
      errMsg = api.asAppError(e).message;
    } finally {
      busy = false;
    }
  }
</script>

<div class="wrap">
  <div class="card">
    <h1>Set up Rustify</h1>
    <p class="muted">
      Rustify needs its own Spotify Client ID for library, search and Connect
      traffic. Without one you would share a global quota with every other
      user of this app, which gets rate limited fast. Registering your own
      takes about a minute and is free.
    </p>

    <ol>
      <li>
        Open
        <code>developer.spotify.com/dashboard</code>
        and log in with your Spotify account.
      </li>
      <li><strong>Create app</strong> — any name and description works.</li>
      <li>
        In the app's settings, add this exact <strong>Redirect URI</strong>:
        <code class="block">{info?.webapiRedirectUri ?? "http://127.0.0.1:8899/login"}</code>
      </li>
      <li>Save, then copy the app's <strong>Client ID</strong> and paste it below.</li>
    </ol>

    <input
      type="text"
      placeholder="Client ID"
      bind:value={clientId}
      disabled={busy}
      onkeydown={(e) => e.key === "Enter" && save()}
    />

    <button class="btn-primary" onclick={save} disabled={busy || !clientId.trim()}>
      {busy ? "Saving…" : "Continue"}
    </button>

    <button class="secondary" onclick={onPair} disabled={busy}>
      Pair with a code instead
    </button>

    {#if errMsg}
      <div class="err">
        <strong>Could not save Client ID</strong>
        <span>{errMsg}</span>
      </div>
    {/if}

    <p class="muted small">
      A Client ID is not a secret — it is safe to paste here and does not need
      to be kept private.
    </p>
    <p class="muted small">
      Device pairing needs no redirect URI. Spotify will show a short code that
      you can approve at spotify.com/pair on this or another device.
    </p>
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
    max-width: 460px;
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
  ol {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    line-height: 1.5;
  }
  code {
    font-size: 11px;
    padding: 1px 4px;
    border-radius: 3px;
    background: rgba(255, 241, 224, 0.08);
    word-break: break-all;
  }
  code.block {
    display: inline-block;
    margin-top: 4px;
    padding: 4px 8px;
  }
  input {
    width: 100%;
    padding: 10px 12px;
    border-radius: var(--r-sm);
    background: var(--glass-strong);
    border: 1px solid var(--hairline);
    color: inherit;
    font-size: 13px;
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
</style>
