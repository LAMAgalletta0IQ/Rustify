<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type { AppSettings, LoginInfo } from "../types";

  let { onReconfigure }: { onReconfigure: () => Promise<void> } = $props();

  let draft = $state<AppSettings>({ ...store.settings });
  let loginInfo = $state<LoginInfo | null>(null);
  let saving = $state(false);
  let saved = $state(false);
  let localError = $state<string | null>(null);

  $effect(() => {
    api.getLoginInfo().then((info) => (loginInfo = info)).catch(() => {});
  });

  async function save() {
    saving = true;
    saved = false;
    localError = null;
    try {
      await store.saveSettings({ ...draft });
      draft = { ...store.settings };
      saved = true;
    } catch (e) {
      localError = store.handleError(e, false).message;
    } finally {
      saving = false;
    }
  }
</script>

<div class="settings">
  <header><div><h1>Settings</h1><p class="muted">Preferences that affect Rustify’s actual playback and storage behavior.</p></div></header>

  <section>
    <h2>Playback</h2>
    <label class="field">
      <span><strong>Default volume</strong><small>Used when a new local playback session starts.</small></span>
      <span class="inline"><input type="range" min="0" max="100" bind:value={draft.defaultVolumePercent} aria-label="Default volume" /><output>{draft.defaultVolumePercent}%</output></span>
    </label>
  </section>

  <section>
    <h2>Appearance</h2>
    <label class="field click">
      <span><strong>Reduce ambient motion</strong><small>Stops Rustify’s animated background in addition to the operating-system preference.</small></span>
      <input type="checkbox" bind:checked={draft.reduceMotion} />
    </label>
  </section>

  <section>
    <h2>Storage</h2>
    <label class="field">
      <span><strong>Audio cache limit</strong><small>Applied when the next playback session starts. Allowed range: 128–8192 MB.</small></span>
      <span class="inline"><input class="number" type="number" min="128" max="8192" step="128" bind:value={draft.cacheLimitMb} /><span>MB</span></span>
    </label>
  </section>

  <section>
    <h2>Spotify integration</h2>
    <div class="field">
      <span><strong>{loginInfo?.privateClientId ? "Client ID configured" : "Client ID missing"}</strong><small>Web API requests use the Spotify app configured during setup. The client ID is not a secret.</small></span>
      <button class="secondary" onclick={onReconfigure}>Replace integration</button>
    </div>
    <p class="warning">Replacing the integration signs out the current session so the new Spotify app can request its own OAuth grant.</p>
  </section>

  <div class="actions">
    <button class="btn-primary" disabled={saving} onclick={save}>{saving ? "Saving…" : "Save settings"}</button>
    {#if saved}<span class="ok" role="status">Saved</span>{/if}
    {#if localError}<span class="error" role="alert">{localError}</span>{/if}
  </div>
</div>

<style>
  .settings { max-width: 780px; margin: 0 auto; padding: 22px 0 40px; }
  header { margin-bottom: 24px; }
  h1 { margin: 0 0 5px; font-size: 27px; }
  header p { margin: 0; }
  section { margin-top: 14px; padding: 18px 20px; border: 1px solid var(--hairline); background: var(--glass); border-radius: var(--r-md); backdrop-filter: blur(var(--blur)); }
  h2 { margin: 0 0 14px; font-size: 15px; }
  .field { display: flex; align-items: center; justify-content: space-between; gap: 28px; }
  .field > span:first-child { display: flex; flex-direction: column; gap: 4px; }
  small { color: var(--fg-dim); line-height: 1.4; }
  .inline { display: flex; align-items: center; gap: 10px; flex: none; }
  output { min-width: 38px; text-align: right; font-variant-numeric: tabular-nums; }
  input[type="range"] { width: 180px; }
  input[type="checkbox"] { width: 18px; height: 18px; accent-color: var(--accent); }
  .number { width: 96px; padding: 8px 10px; border-radius: var(--r-sm); color: var(--fg); background: var(--glass-strong); border: 1px solid var(--hairline); }
  .secondary { padding: 8px 12px; border-radius: var(--r-sm); border: 1px solid var(--hairline); background: var(--glass-strong); }
  .warning { margin: 12px 0 0; color: var(--fg-dim); font-size: 11px; }
  .actions { display: flex; align-items: center; gap: 14px; margin-top: 18px; }
  .ok { color: var(--accent); }
  .error { color: #ff9a9a; }
  @media (max-width: 620px) { .field { align-items: flex-start; flex-direction: column; gap: 14px; } }
</style>
