<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import * as api from "../../api";
  import AudioOutputSelector from "../audio/AudioOutputSelector.svelte";
  import { audioDevices } from "../audio/audio-devices.svelte";
  import { store } from "../../store.svelte";
  import SelectMenu, { type SelectOption } from "../../ui/SelectMenu.svelte";
  import type { AppSettings, EqualizerPreset, LoginInfo, TelemetryStatus } from "../../types";

  let { onReconfigure }: { onReconfigure: () => Promise<void> } = $props();
  const bands = [
    ["60 Hz", "60 hertz"], ["150 Hz", "150 hertz"], ["400 Hz", "400 hertz"],
    ["1 kHz", "1 kilohertz"], ["2.4 kHz", "2.4 kilohertz"], ["15 kHz", "15 kilohertz"],
  ];
  const qualityOptions: SelectOption[] = [
    { value: "automatic", label: "Automatic · 160 kbps", description: "Balanced default" },
    { value: "low", label: "Data saver · 96 kbps", description: "Lowest bandwidth" },
    { value: "normal", label: "Normal · 160 kbps", description: "Consistent quality" },
    { value: "veryHigh", label: "Very high · 320 kbps", description: "Highest available quality" },
  ];

  let draft = $state<AppSettings>(cloneSettings(store.settings));
  let loginInfo = $state<LoginInfo | null>(null);
  let telemetry = $state<TelemetryStatus | null>(null);
  let builtins = $state<EqualizerPreset[]>([]);
  let presetName = $state("");
  let saving = $state(false);
  let saved = $state(false);
  let audioState = $state<"idle" | "applying" | "saved" | "error">("idle");
  let localError = $state<string | null>(null);
  let audioTimer: number | null = null;
  let audioRevision = 0;

  const allPresets = $derived([...builtins, ...draft.equalizer.customPresets]);
  const presetOptions = $derived<SelectOption[]>(allPresets.map((preset) => ({
    value: preset.id, label: preset.name,
    description: preset.id.startsWith("custom-") ? "Custom" : "Built in",
  })));
  const curvePoints = $derived(draft.equalizer.bandsDb.map((gain, index) =>
    `${8 + index * 16.8},${32 - gain * (22 / 12)}`).join(" "));
  /** Node positions in the same 0-100/0-64 space as the SVG curve above, but
   * expressed as CSS percentages so plain HTML buttons (not SVG shapes,
   * which can't take keyboard focus on their own) can sit on top of it. */
  const nodePositions = $derived(draft.equalizer.bandsDb.map((gain, index) => ({
    gain,
    left: 8 + index * 16.8,
    top: ((32 - gain * (22 / 12)) / 64) * 100,
  })));

  let graphEl = $state<HTMLElement | null>(null);
  let draggingIndex = $state<number | null>(null);

  function gainFromClientY(clientY: number): number {
    if (!graphEl) return 0;
    const rect = graphEl.getBoundingClientRect();
    const ratio = (clientY - rect.top) / rect.height;
    const gain = 12 - ratio * 24;
    return Math.max(-12, Math.min(12, Math.round(gain * 2) / 2));
  }

  function startDrag(event: PointerEvent, index: number) {
    if (!draft.equalizer.enabled) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).focus();
    draggingIndex = index;
    setBand(index, gainFromClientY(event.clientY));
    const move = (moveEvent: PointerEvent) => {
      if (draggingIndex === null) return;
      setBand(draggingIndex, gainFromClientY(moveEvent.clientY));
    };
    const up = () => {
      draggingIndex = null;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function nodeKeydown(event: KeyboardEvent, index: number) {
    const gain = draft.equalizer.bandsDb[index];
    if (event.key === "ArrowUp" || event.key === "ArrowRight") {
      event.preventDefault();
      setBand(index, Math.min(12, gain + 0.5));
    } else if (event.key === "ArrowDown" || event.key === "ArrowLeft") {
      event.preventDefault();
      setBand(index, Math.max(-12, gain - 0.5));
    } else if (event.key === "Home") {
      event.preventDefault();
      setBand(index, 12);
    } else if (event.key === "End") {
      event.preventDefault();
      setBand(index, -12);
    } else if (event.key === "0") {
      event.preventDefault();
      setBand(index, 0);
    }
  }

  function cloneSettings(settings: AppSettings): AppSettings {
    return { ...settings, equalizer: { ...settings.equalizer,
      bandsDb: [...settings.equalizer.bandsDb],
      customPresets: settings.equalizer.customPresets.map((preset) => ({ ...preset, bandsDb: [...preset.bandsDb] })),
    }} as AppSettings;
  }

  function audioSnapshot() {
    return { outputDevice: draft.outputDevice, equalizer: { ...draft.equalizer,
      bandsDb: [...draft.equalizer.bandsDb] as AppSettings["equalizer"]["bandsDb"],
      customPresets: draft.equalizer.customPresets.map((preset) => ({
        ...preset, bandsDb: [...preset.bandsDb] as EqualizerPreset["bandsDb"],
      })),
    }};
  }

  async function persistAudio(snapshot: ReturnType<typeof audioSnapshot>, revision: number) {
    try {
      await store.saveAudioSettings(snapshot.outputDevice, snapshot.equalizer);
      if (revision === audioRevision) audioState = "saved";
    } catch (error) {
      if (revision === audioRevision) {
        audioState = "error";
        localError = store.handleError(error, false).message;
      }
    }
  }

  function applyAudio() {
    const snapshot = audioSnapshot();
    const revision = ++audioRevision;
    audioState = "applying";
    localError = null;
    void api.configureAudio(snapshot.outputDevice, snapshot.equalizer).catch((error) => {
      if (revision === audioRevision) {
        audioState = "error";
        localError = store.handleError(error, false).message;
      }
    });
    if (audioTimer !== null) window.clearTimeout(audioTimer);
    audioTimer = window.setTimeout(() => {
      audioTimer = null;
      void persistAudio(snapshot, revision);
    }, 220);
  }

  onMount(() => {
    void Promise.all([api.getLoginInfo(), api.getEqualizerPresets(), api.getTelemetryStatus()])
      .then(([info, presets, telemetryStatus]) => { loginInfo = info; builtins = presets; telemetry = telemetryStatus; })
      .catch((error) => { localError = store.handleError(error, false).message; });
  });
  onDestroy(() => {
    if (audioTimer !== null) {
      window.clearTimeout(audioTimer);
      void persistAudio(audioSnapshot(), ++audioRevision);
    }
  });

  function setBand(index: number, value: number) {
    const values = [...draft.equalizer.bandsDb] as AppSettings["equalizer"]["bandsDb"];
    values[index] = value;
    draft.equalizer.bandsDb = values;
    draft.equalizer.activePresetId = null;
    applyAudio();
  }
  function setPreamp(value: number) {
    draft.equalizer.preampDb = value;
    draft.equalizer.activePresetId = null;
    applyAudio();
  }
  function applyPresetById(id: string | null) {
    const preset = allPresets.find((item) => item.id === id);
    if (!preset) return;
    draft.equalizer.bandsDb = [...preset.bandsDb];
    draft.equalizer.preampDb = preset.preampDb;
    draft.equalizer.activePresetId = preset.id;
    draft.equalizer.enabled = true;
    applyAudio();
  }
  function resetEqualizer() {
    const flat = builtins.find((preset) => preset.id === "flat");
    draft.equalizer.bandsDb = flat ? [...flat.bandsDb] : [0, 0, 0, 0, 0, 0];
    draft.equalizer.preampDb = 0;
    draft.equalizer.autoHeadroom = true;
    draft.equalizer.activePresetId = "flat";
    draft.equalizer.enabled = false;
    applyAudio();
  }
  function createPreset() {
    const name = presetName.trim();
    if (!name) return;
    const preset: EqualizerPreset = { id: `custom-${crypto.randomUUID()}`, name,
      bandsDb: [...draft.equalizer.bandsDb], preampDb: draft.equalizer.preampDb };
    draft.equalizer.customPresets = [...draft.equalizer.customPresets, preset];
    draft.equalizer.activePresetId = preset.id;
    presetName = "";
    applyAudio();
  }
  function deletePreset(id: string) {
    draft.equalizer.customPresets = draft.equalizer.customPresets.filter((preset) => preset.id !== id);
    if (draft.equalizer.activePresetId === id) draft.equalizer.activePresetId = null;
    applyAudio();
  }
  async function save() {
    saving = true; saved = false; localError = null;
    try {
      await store.saveSettings(cloneSettings(draft));
      draft = cloneSettings(store.settings);
      saved = true;
    } catch (error) { localError = store.handleError(error, false).message; }
    finally { saving = false; }
  }
</script>

<div class="settings">
  <header><h1>Settings</h1><p class="muted">Playback, sound, and storage controls backed by Rustify’s native audio pipeline.</p></header>

  <section>
    <h2>Playback</h2>
    <div class="field"><span><strong>Streaming quality</strong><small>Applied when the next local playback session starts.</small></span><SelectMenu bind:value={draft.audioQuality} options={qualityOptions} label="Streaming quality" /></div>
    <p class="notice">The installed librespot 0.8 player requests 96, 160, or 320 kbps lossy streams. Rustify does not label these streams as lossless.</p>
    <label class="field">
      <span><strong>Crossfade</strong><small>Equal-power decoder overlap between consecutive tracks. Applied to the next local playback session; turn it off for spoken-word listening.</small></span>
      <span class="range-row"><input type="range" min="0" max="12" step="1" bind:value={draft.crossfadeSeconds} aria-label="Crossfade duration" /><output>{draft.crossfadeSeconds === 0 ? "Off" : `${draft.crossfadeSeconds}s`}</output></span>
    </label>
  </section>

  <section>
    <div class="section-title"><div><h2>Audio output</h2><small>Hot-plug changes refresh automatically. Switching preserves the playback session and queue.</small></div><button class="secondary" disabled={audioDevices.loading} onclick={() => void audioDevices.refresh()}>{audioDevices.loading ? "Refreshing…" : "Refresh"}</button></div>
    <div class="field"><span><strong>Output device</strong><small>{audioDevices.status?.activeDevice ? `Active: ${audioDevices.status.activeDevice}` : "Following the system default."}</small></span><AudioOutputSelector bind:value={draft.outputDevice} onChange={() => applyAudio()} /></div>
  </section>

  <section class="equalizer">
    <div class="section-title"><div><h2>Equalizer</h2><small>Six smoothed peaking filters run in the active native audio sink.</small></div><label class="toggle"><input type="checkbox" checked={draft.equalizer.enabled} onchange={(event) => { draft.equalizer.enabled = event.currentTarget.checked; applyAudio(); }} /><span aria-hidden="true"></span><strong>{draft.equalizer.enabled ? "Enabled" : "Bypassed"}</strong></label></div>
    <div class="preset-control"><span><strong>Preset</strong><small>Presets apply to playback immediately.</small></span><SelectMenu value={draft.equalizer.activePresetId} options={presetOptions} label="Equalizer preset" onChange={applyPresetById} /></div>

    <div class="graph-wrap" class:bypassed={!draft.equalizer.enabled} bind:this={graphEl}>
      <span class="scale top">+12</span><span class="scale zero">0</span><span class="scale bottom">−12 dB</span>
      <svg class="curve" viewBox="0 0 100 64" preserveAspectRatio="none" aria-hidden="true">
        <line x1="0" y1="10" x2="100" y2="10" /><line x1="0" y1="32" x2="100" y2="32" /><line x1="0" y1="54" x2="100" y2="54" />
        <polyline points={curvePoints} />
      </svg>
      <div class="nodes" role="group" aria-label="Equalizer bands">
        {#each nodePositions as node, index (index)}
          <button
            class="node"
            class:dragging={draggingIndex === index}
            style={`left:${node.left}%;top:${node.top}%`}
            disabled={!draft.equalizer.enabled}
            role="slider"
            aria-label={`${bands[index][1]} gain`}
            aria-valuemin="-12"
            aria-valuemax="12"
            aria-valuenow={node.gain}
            aria-valuetext={`${node.gain > 0 ? "+" : ""}${node.gain.toFixed(1)} decibels`}
            onpointerdown={(event) => startDrag(event, index)}
            onkeydown={(event) => nodeKeydown(event, index)}
          ><span class="node-value">{node.gain > 0 ? "+" : ""}{node.gain.toFixed(1)}</span></button>
        {/each}
      </div>
    </div>
    <div class="freq-labels">
      {#each bands as [label]}<span>{label}</span>{/each}
    </div>
    <div class="field preamp"><span><strong>Preamp</strong><small>Automatic headroom subtracts the largest boost to reduce clipping risk.</small></span><span class="range-row"><input type="range" min="-12" max="0" step="0.5" value={draft.equalizer.preampDb} oninput={(event) => setPreamp(Number(event.currentTarget.value))} aria-label="Equalizer preamp" /><output>{draft.equalizer.preampDb.toFixed(1)} dB</output></span></div>
    <label class="check"><input type="checkbox" checked={draft.equalizer.autoHeadroom} onchange={(event) => { draft.equalizer.autoHeadroom = event.currentTarget.checked; applyAudio(); }} /> Automatic headroom compensation</label>
    <div class="preset-tools"><input bind:value={presetName} maxlength="48" placeholder="Custom preset name" aria-label="Custom preset name" /><button class="secondary" disabled={!presetName.trim()} onclick={createPreset}>Save curve</button><button class="secondary" onclick={resetEqualizer}>Reset</button></div>
    {#if draft.equalizer.customPresets.length}<div class="custom-list" aria-label="Custom equalizer presets">{#each draft.equalizer.customPresets as preset (preset.id)}<span><strong>{preset.name}</strong><button aria-label={`Delete ${preset.name}`} onclick={() => deletePreset(preset.id)}>Delete</button></span>{/each}</div>{/if}
    <p class="audio-status" class:error={audioState === "error"} role="status">{audioState === "applying" ? "Applying to the audio pipeline…" : audioState === "saved" ? "Active and saved." : audioState === "error" ? "Audio changes could not be applied." : "Adjustments are applied live and saved automatically."}</p>
  </section>

  <section><h2>Appearance</h2><label class="field click"><span><strong>Reduce ambient motion</strong><small>Also respects the operating system’s reduced-motion preference.</small></span><input type="checkbox" bind:checked={draft.reduceMotion} /></label></section>
  <section><h2>Storage</h2><label class="field"><span><strong>Audio cache limit</strong><small>Applied to the next playback session. Allowed range: 128–8192 MB.</small></span><span class="number-row"><input type="number" min="128" max="8192" step="128" bind:value={draft.cacheLimitMb} aria-label="Audio cache limit in megabytes" /><span>MB</span></span></label></section>
  <section><h2>Spotify integration</h2><div class="field"><span><strong>{loginInfo?.privateClientId ? "Client ID configured" : "Client ID missing"}</strong><small>Web API requests use the Spotify app configured during setup. The client ID is not a secret.</small></span><button class="secondary" onclick={onReconfigure}>Replace integration</button></div><p class="notice">Replacing the integration signs out the current session so the new Spotify app can request its own OAuth grant.</p>
    <div class="field"><span><strong>Playback history delivery {telemetry?.deliveryAvailable ? "available" : "unavailable"}</strong><small>{telemetry?.deliveryAvailable ? `Using ${telemetry.deliveryTransport}.` : (telemetry?.deliveryBlocker ?? "Checking Spotify telemetry capability…")}</small></span><span class="audit">{telemetry?.activePlaybacks ?? 0} active · {telemetry?.locallyRecorded ?? 0} audited</span></div>
  </section>
  <div class="actions"><button class="btn-primary" disabled={saving} onclick={save}>{saving ? "Saving…" : "Save general settings"}</button>{#if saved}<span class="ok" role="status">Saved.</span>{/if}{#if localError}<span class="error" role="alert">{localError}</span>{/if}</div>
</div>

<style>
  .settings{max-width:920px;margin:0 auto;padding:22px 0 42px}header{margin-bottom:24px}h1{margin:0 0 5px;font-size:27px}header p,h2,.audio-status{margin:0}section{margin-top:14px;padding:19px 20px;border:1px solid var(--hairline);background:var(--glass);border-radius:var(--r-md);backdrop-filter:blur(var(--blur))}h2{font-size:15px}.section-title,.field,.preset-control{display:flex;align-items:center;justify-content:space-between;gap:28px}.section-title{margin-bottom:16px}section>h2{margin-bottom:15px}.field>span:first-child,.section-title>div,.preset-control>span:first-child{display:flex;min-width:0;flex-direction:column;gap:4px}small,.notice{color:var(--fg-dim);line-height:1.45}.notice{margin:14px 0 0;font-size:11px}.secondary{padding:8px 12px;border:1px solid var(--control-border);border-radius:var(--control-radius);background:var(--control-bg)}.secondary:hover:not(:disabled){background:var(--control-hover);border-color:var(--control-border-hover)}
  .toggle{display:flex;align-items:center;gap:8px;cursor:pointer}.toggle input{position:absolute;opacity:0;pointer-events:none}.toggle>span{position:relative;width:36px;height:20px;border-radius:999px;background:rgba(255,241,224,.16);transition:background var(--motion-fast)}.toggle>span::after{content:"";position:absolute;top:3px;left:3px;width:14px;height:14px;border-radius:50%;background:var(--fg);transition:transform var(--motion-fast)}.toggle input:checked+span{background:var(--accent)}.toggle input:checked+span::after{transform:translateX(16px)}.toggle input:focus-visible+span{box-shadow:var(--focus-ring)}.preset-control{padding:12px 14px;border-radius:var(--r-sm);background:rgba(20,14,8,.2)}
  .graph-wrap{position:relative;height:200px;margin:18px 0 0;padding-left:44px;transition:opacity var(--motion-normal);touch-action:none}.graph-wrap.bypassed{opacity:.45}.curve{position:absolute;inset:0;left:44px;width:calc(100% - 44px);height:100%;overflow:visible;pointer-events:none}.curve line{stroke:rgba(255,241,224,.11);stroke-width:.35;vector-effect:non-scaling-stroke}.curve polyline{fill:none;stroke:var(--accent);stroke-width:2.5;stroke-linecap:round;stroke-linejoin:round;vector-effect:non-scaling-stroke}.scale{position:absolute;left:0;color:var(--fg-faint);font-size:10px;font-variant-numeric:tabular-nums}.scale.top{top:12.6%}.scale.zero{top:50%;transform:translateY(-50%)}.scale.bottom{top:84.4%}
  .nodes{position:absolute;inset:0;left:44px;width:calc(100% - 44px);height:100%}
  .node{position:absolute;display:grid;place-items:center;width:26px;height:26px;margin:-13px 0 0 -13px;border-radius:50%;color:var(--ink);background:var(--accent);border:2px solid rgba(18,11,4,.85);box-shadow:0 2px 8px rgba(0,0,0,.35);cursor:grab;touch-action:none;transition:transform var(--motion-fast)}
  .node:hover:not(:disabled){transform:scale(1.12)}
  .node.dragging{cursor:grabbing;transform:scale(1.18)}
  .node:focus-visible{outline:none;box-shadow:var(--focus-ring),0 2px 8px rgba(0,0,0,.35)}
  .node:disabled{cursor:default;opacity:.55}
  .node-value{position:absolute;top:-22px;font-size:10px;font-variant-numeric:tabular-nums;color:var(--fg-dim);pointer-events:none}
  .freq-labels{display:grid;grid-template-columns:repeat(6,1fr);margin:8px 0 0 44px;color:var(--fg-dim);font-size:11px;text-align:center}
  .preamp{margin-top:20px;padding-top:18px;border-top:1px solid var(--hairline)}.range-row,.number-row{display:flex;align-items:center;gap:10px;flex:none}.range-row input{width:180px;accent-color:var(--accent)}output{min-width:54px;text-align:right;font-variant-numeric:tabular-nums}.check{display:flex;align-items:center;gap:8px;margin-top:14px;font-size:12px;cursor:pointer}input[type=checkbox]{width:18px;height:18px;accent-color:var(--accent)}.preset-tools{display:flex;gap:8px;margin-top:16px}.preset-tools input,.number-row input{min-height:var(--control-height);padding:8px 11px;border:1px solid var(--control-border);border-radius:var(--control-radius);background:var(--control-bg)}.preset-tools input{flex:1}.number-row input{width:96px}.custom-list{display:flex;flex-direction:column;gap:6px;margin-top:10px}.custom-list span{display:flex;align-items:center;gap:8px;padding:7px 9px;border-radius:8px;background:rgba(20,14,8,.2)}.custom-list button{margin-left:auto;color:var(--fg-dim)}.custom-list button:hover{color:var(--warning)}.audio-status{min-height:18px;margin-top:12px;color:var(--fg-dim);font-size:11px}.audio-status.error,.error{color:#ffaaa2}.actions{display:flex;align-items:center;gap:14px;margin-top:18px}.ok{color:var(--accent)}.audit{flex:none;color:var(--fg-dim);font-size:11px;font-variant-numeric:tabular-nums}
  @media(max-width:680px){.field,.preset-control{align-items:flex-start;flex-direction:column;gap:14px}.graph-wrap{padding-left:38px}.curve,.nodes{left:38px;width:calc(100% - 38px)}.freq-labels{margin-left:38px;font-size:10px}.preset-tools{flex-wrap:wrap}}
</style>
