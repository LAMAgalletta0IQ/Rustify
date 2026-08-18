<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type { AppSettings, AudioDevice, AudioStatus, EqualizerPreset, LoginInfo } from "../types";

  let { onReconfigure }: { onReconfigure: () => Promise<void> } = $props();

  const frequencies = ["60", "150", "400", "1k", "2.4k", "15k"];
  let draft = $state<AppSettings>(cloneSettings(store.settings));
  let loginInfo = $state<LoginInfo | null>(null);
  let devices = $state<AudioDevice[]>([]);
  let audioStatus = $state<AudioStatus | null>(null);
  let builtins = $state<EqualizerPreset[]>([]);
  let presetName = $state("");
  let loadingDevices = $state(false);
  let saving = $state(false);
  let saved = $state(false);
  let localError = $state<string | null>(null);

  function cloneSettings(settings: AppSettings): AppSettings {
    return {
      ...settings,
      equalizer: {
        ...settings.equalizer,
        bandsDb: [...settings.equalizer.bandsDb],
        customPresets: settings.equalizer.customPresets.map((preset) => ({
          ...preset,
          bandsDb: [...preset.bandsDb],
        })),
      },
    } as AppSettings;
  }

  async function refreshDevices() {
    loadingDevices = true;
    try {
      [devices, audioStatus] = await Promise.all([api.listAudioDevices(), api.getAudioStatus()]);
    } catch (e) {
      localError = store.handleError(e, false).message;
    } finally {
      loadingDevices = false;
    }
  }

  $effect(() => {
    Promise.all([api.getLoginInfo(), api.getEqualizerPresets()])
      .then(([info, presets]) => {
        loginInfo = info;
        builtins = presets;
      })
      .catch(() => {});
  });

  onMount(() => {
    void refreshDevices();
    const timer = window.setInterval(refreshDevices, 10_000);
    return () => window.clearInterval(timer);
  });

  function setBand(index: number, value: number) {
    const bands = [...draft.equalizer.bandsDb] as AppSettings["equalizer"]["bandsDb"];
    bands[index] = value;
    draft.equalizer.bandsDb = bands;
    draft.equalizer.activePresetId = null;
  }

  function applyPreset(preset: EqualizerPreset) {
    draft.equalizer.bandsDb = [...preset.bandsDb];
    draft.equalizer.preampDb = preset.preampDb;
    draft.equalizer.activePresetId = preset.id;
    draft.equalizer.enabled = true;
  }

  function resetEqualizer() {
    const flat = builtins.find((preset) => preset.id === "flat");
    if (flat) applyPreset(flat);
    draft.equalizer.enabled = false;
  }

  function createPreset() {
    const name = presetName.trim();
    if (!name) return;
    const id = `custom-${crypto.randomUUID()}`;
    const preset: EqualizerPreset = {
      id,
      name,
      bandsDb: [...draft.equalizer.bandsDb],
      preampDb: draft.equalizer.preampDb,
    };
    draft.equalizer.customPresets = [...draft.equalizer.customPresets, preset];
    draft.equalizer.activePresetId = id;
    presetName = "";
  }

  function renamePreset(preset: EqualizerPreset) {
    const name = window.prompt("Preset name", preset.name)?.trim();
    if (!name) return;
    draft.equalizer.customPresets = draft.equalizer.customPresets.map((item) =>
      item.id === preset.id ? { ...item, name } : item,
    );
  }

  function deletePreset(id: string) {
    draft.equalizer.customPresets = draft.equalizer.customPresets.filter((item) => item.id !== id);
    if (draft.equalizer.activePresetId === id) draft.equalizer.activePresetId = null;
  }

  async function save() {
    saving = true;
    saved = false;
    localError = null;
    try {
      await store.saveSettings(cloneSettings(draft));
      draft = cloneSettings(store.settings);
      audioStatus = await api.getAudioStatus();
      saved = true;
    } catch (e) {
      localError = store.handleError(e, false).message;
    } finally {
      saving = false;
    }
  }
</script>

<div class="settings">
  <header><div><h1>Settings</h1><p class="muted">Playback, sound, and storage controls backed by Rustify’s native audio pipeline.</p></div></header>

  <section>
    <h2>Playback</h2>
    <label class="field">
      <span><strong>Default volume</strong><small>Used when a new local playback session starts.</small></span>
      <span class="inline"><input type="range" min="0" max="100" bind:value={draft.defaultVolumePercent} aria-label="Default volume" /><output>{draft.defaultVolumePercent}%</output></span>
    </label>
    <label class="field">
      <span><strong>Streaming quality</strong><small>Applied to the next local playback session. Automatic currently resolves to 160 kbps.</small></span>
      <select bind:value={draft.audioQuality} aria-label="Streaming quality">
        <option value="automatic">Automatic · 160 kbps</option>
        <option value="low">Data saver · 96 kbps</option>
        <option value="normal">Normal · 160 kbps</option>
        <option value="veryHigh">Very high · 320 kbps</option>
      </select>
    </label>
    <p class="notice">Lossless is not offered: the installed librespot 0.8 player selects only 96, 160, or 320 kbps lossy streams. Rustify never labels those streams as lossless.</p>
  </section>

  <section>
    <div class="section-title"><div><h2>Audio output</h2><small>Switching preserves the current queue, position, and session.</small></div><button class="secondary" disabled={loadingDevices} onclick={refreshDevices}>{loadingDevices ? "Refreshing…" : "Refresh"}</button></div>
    <label class="field">
      <span><strong>Output device</strong><small>{audioStatus?.activeDevice ? `Active: ${audioStatus.activeDevice}` : "The system default is used until local playback starts."}</small></span>
      <select bind:value={draft.outputDevice} aria-label="Audio output device">
        <option value={null}>System default</option>
        {#each devices as device}
          <option value={device.id}>{device.name}{device.isDefault ? " · default" : ""}</option>
        {/each}
      </select>
    </label>
    {#if audioStatus?.lastError}<p class="error" role="status">{audioStatus.lastError}</p>{/if}
  </section>

  <section class="equalizer">
    <div class="section-title"><div><h2>Equalizer</h2><small>Six real peaking filters run on the dedicated audio sink thread.</small></div><label class="switch"><input type="checkbox" bind:checked={draft.equalizer.enabled} /><span>{draft.equalizer.enabled ? "On" : "Bypassed"}</span></label></div>
    <div class="preset-row" aria-label="Equalizer presets">
      {#each [...builtins, ...draft.equalizer.customPresets] as preset}
        <button class:active={draft.equalizer.activePresetId === preset.id} onclick={() => applyPreset(preset)}>{preset.name}</button>
      {/each}
    </div>
    <svg class="curve" viewBox="0 0 600 120" role="img" aria-label="Active equalizer band curve">
      <line x1="0" y1="60" x2="600" y2="60" />
      <polyline points={draft.equalizer.bandsDb.map((gain, index) => `${index * 120},${60 - gain * 4}`).join(" ")} />
    </svg>
    <div class="bands">
      {#each draft.equalizer.bandsDb as gain, index}
        <label><output>{gain > 0 ? "+" : ""}{gain.toFixed(1)}</output><input type="range" min="-12" max="12" step="0.5" value={gain} oninput={(event) => setBand(index, Number(event.currentTarget.value))} aria-label={`${frequencies[index]} hertz gain`} /><span>{frequencies[index]} Hz</span></label>
      {/each}
    </div>
    <label class="field compact"><span><strong>Preamp / headroom</strong><small>Automatic headroom subtracts the largest boost to reduce clipping risk.</small></span><span class="inline"><input type="range" min="-12" max="0" step="0.5" bind:value={draft.equalizer.preampDb} aria-label="Equalizer preamp" /><output>{draft.equalizer.preampDb.toFixed(1)} dB</output></span></label>
    <label class="check"><input type="checkbox" bind:checked={draft.equalizer.autoHeadroom} /> Automatic headroom compensation</label>
    <div class="preset-tools"><input bind:value={presetName} maxlength="48" placeholder="Custom preset name" aria-label="Custom preset name" /><button class="secondary" disabled={!presetName.trim()} onclick={createPreset}>Save current curve</button><button class="secondary" onclick={resetEqualizer}>Reset</button></div>
    {#if draft.equalizer.customPresets.length}
      <div class="custom-list">
        {#each draft.equalizer.customPresets as preset}
          <span>{preset.name}<button aria-label={`Rename ${preset.name}`} onclick={() => renamePreset(preset)}>Rename</button><button aria-label={`Delete ${preset.name}`} onclick={() => deletePreset(preset.id)}>Delete</button></span>
        {/each}
      </div>
    {/if}
  </section>

  <section>
    <h2>Appearance</h2>
    <label class="field click"><span><strong>Reduce ambient motion</strong><small>Also respects the operating system’s reduced-motion preference.</small></span><input type="checkbox" bind:checked={draft.reduceMotion} /></label>
  </section>

  <section>
    <h2>Storage</h2>
    <label class="field"><span><strong>Audio cache limit</strong><small>Applied when the next playback session starts. Allowed range: 128–8192 MB.</small></span><span class="inline"><input class="number" type="number" min="128" max="8192" step="128" bind:value={draft.cacheLimitMb} /><span>MB</span></span></label>
  </section>

  <section>
    <h2>Spotify integration</h2>
    <div class="field"><span><strong>{loginInfo?.privateClientId ? "Client ID configured" : "Client ID missing"}</strong><small>Web API requests use the Spotify app configured during setup. The client ID is not a secret.</small></span><button class="secondary" onclick={onReconfigure}>Replace integration</button></div>
    <p class="notice">Replacing the integration signs out the current session so the new Spotify app can request its own OAuth grant.</p>
  </section>

  <div class="actions"><button class="btn-primary" disabled={saving} onclick={save}>{saving ? "Saving…" : "Save settings"}</button>{#if saved}<span class="ok" role="status">Saved. Output and equalizer changes are active; quality applies next session.</span>{/if}{#if localError}<span class="error" role="alert">{localError}</span>{/if}</div>
</div>

<style>
  .settings { max-width: 900px; margin: 0 auto; padding: 22px 0 40px; }
  header { margin-bottom: 24px; } h1 { margin: 0 0 5px; font-size: 27px; } header p, h2 { margin: 0; }
  section { margin-top: 14px; padding: 18px 20px; border: 1px solid var(--hairline); background: var(--glass); border-radius: var(--r-md); backdrop-filter: blur(var(--blur)); }
  h2 { font-size: 15px; }
  .section-title, .field { display: flex; align-items: center; justify-content: space-between; gap: 28px; }
  .section-title { margin-bottom: 14px; } section > h2 { margin-bottom: 14px; }
  .field + .field { margin-top: 17px; padding-top: 17px; border-top: 1px solid var(--hairline); }
  .field > span:first-child, .section-title > div { display: flex; flex-direction: column; gap: 4px; }
  small, .notice { color: var(--fg-dim); line-height: 1.45; }
  .notice { margin: 14px 0 0; font-size: 11px; }
  .inline { display: flex; align-items: center; gap: 10px; flex: none; }
  output { min-width: 48px; text-align: right; font-variant-numeric: tabular-nums; }
  input[type="range"] { width: 180px; accent-color: var(--accent); }
  input[type="checkbox"] { width: 18px; height: 18px; accent-color: var(--accent); }
  select, .number, .preset-tools input { padding: 8px 10px; border-radius: var(--r-sm); color: var(--fg); background: var(--glass-strong); border: 1px solid var(--hairline); }
  select { max-width: 330px; }
  .number { width: 96px; }
  .secondary, .preset-row button, .custom-list button { padding: 8px 12px; border-radius: var(--r-sm); border: 1px solid var(--hairline); background: var(--glass-strong); }
  .switch, .check { display: flex; align-items: center; gap: 8px; }
  .preset-row { display: flex; flex-wrap: wrap; gap: 7px; }
  .preset-row button.active { color: var(--accent); border-color: color-mix(in srgb, var(--accent) 55%, transparent); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .curve { width: 100%; height: 110px; margin: 15px 0 4px; overflow: visible; }
  .curve line { stroke: var(--hairline); } .curve polyline { fill: none; stroke: var(--accent); stroke-width: 3; stroke-linecap: round; stroke-linejoin: round; vector-effect: non-scaling-stroke; }
  .bands { display: grid; grid-template-columns: repeat(6, 1fr); gap: 10px; }
  .bands label { display: flex; flex-direction: column; align-items: center; gap: 6px; color: var(--fg-dim); font-size: 11px; }
  .bands input { width: 100%; }
  .bands output { color: var(--fg); text-align: center; }
  .compact { margin-top: 18px; padding-top: 16px; border-top: 1px solid var(--hairline); }
  .check { margin-top: 14px; font-size: 12px; }
  .preset-tools { display: flex; gap: 8px; margin-top: 15px; }
  .preset-tools input { flex: 1; }
  .custom-list { display: flex; flex-direction: column; gap: 6px; margin-top: 10px; }
  .custom-list span { display: flex; align-items: center; gap: 6px; color: var(--fg-dim); }
  .custom-list button:first-of-type { margin-left: auto; }
  .custom-list button { padding: 5px 8px; font-size: 11px; }
  .actions { display: flex; align-items: center; gap: 14px; margin-top: 18px; }
  .ok { color: var(--accent); } .error { color: #ff9a9a; }
  @media (max-width: 620px) { .field { align-items: flex-start; flex-direction: column; gap: 14px; } .bands { grid-template-columns: repeat(3, 1fr); } .preset-tools { flex-wrap: wrap; } }
</style>
