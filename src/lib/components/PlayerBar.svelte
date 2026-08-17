<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import { formatMs, volumeToPercent } from "../types";
  import DevicePicker from "./DevicePicker.svelte";

  let { onOpenNowPlaying }: { onOpenNowPlaying: () => void } = $props();

  const pb = $derived(store.playback);
  const pct = $derived(
    pb.durationMs > 0 ? (pb.positionMs / pb.durationMs) * 100 : 0,
  );

  function onSeek(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    store.run(() => api.seek(Math.round((v / 100) * pb.durationMs)));
  }

  function onVolume(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    store.run(() => api.setVolume(v));
  }

  function cycleRepeat() {
    // off -> context -> track -> off
    const [c, t] = pb.repeatTrack
      ? [false, false]
      : pb.repeatContext
        ? [false, true]
        : [true, false];
    store.run(() => api.setRepeat(c, t));
  }
</script>

<footer>
  <button class="now" onclick={onOpenNowPlaying} title="Open now playing">
    {#if pb.track?.coverUrl}
      <img src={pb.track.coverUrl} alt="" width="52" height="52" />
    {:else}
      <span class="ph"></span>
    {/if}
    <span class="meta">
      <span class="name truncate">{pb.track?.name ?? "Nothing playing"}</span>
      <span class="artist muted truncate"
        >{pb.track?.artists.join(", ") ?? ""}</span
      >
    </span>
  </button>

  <div class="center">
    <div class="controls">
      <button
        class:on={pb.shuffle}
        onclick={() => store.run(() => api.setShuffle(!pb.shuffle))}
        title="Shuffle">⤨</button
      >
      <button onclick={() => store.run(api.previousTrack)} title="Previous"
        >⏮</button
      >
      <button
        class="pp"
        onclick={() => store.run(api.playPause)}
        title={pb.isPlaying ? "Pause" : "Play"}
      >
        {pb.isLoading ? "…" : pb.isPlaying ? "⏸" : "▶"}
      </button>
      <button onclick={() => store.run(api.nextTrack)} title="Next">⏭</button>
      <button
        class:on={pb.repeatContext || pb.repeatTrack}
        onclick={cycleRepeat}
        title="Repeat">{pb.repeatTrack ? "🔂" : "🔁"}</button
      >
    </div>

    <div class="scrub">
      <span class="t muted">{formatMs(pb.positionMs)}</span>
      <input
        type="range"
        min="0"
        max="100"
        step="0.1"
        value={pct}
        onchange={onSeek}
        disabled={!pb.track}
        aria-label="Seek"
      />
      <span class="t muted">{formatMs(pb.durationMs)}</span>
    </div>
  </div>

  <div class="right">
    <DevicePicker />
    <input
      class="vol"
      type="range"
      min="0"
      max="100"
      value={volumeToPercent(pb.volume)}
      onchange={onVolume}
      aria-label="Volume"
    />
  </div>
</footer>

<style>
  footer {
    display: grid;
    grid-template-columns: minmax(160px, 1fr) minmax(320px, 2fr) minmax(
        160px,
        1fr
      );
    align-items: center;
    gap: 16px;
    padding: 10px 16px;
    background: var(--bg-elev);
    border-top: 1px solid var(--border);
  }
  .now {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    padding: 4px;
    text-align: left;
  }
  img,
  .ph {
    width: 52px;
    height: 52px;
    border-radius: 6px;
    background: var(--bg-elev-2);
    object-fit: cover;
    flex: none;
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .artist {
    font-size: 12px;
  }
  .center {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .controls {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 8px;
  }
  .controls button {
    padding: 4px 8px;
    color: var(--fg-dim);
    font-size: 15px;
  }
  .controls button:hover {
    color: var(--fg);
  }
  .controls button.on {
    color: var(--accent);
  }
  .pp {
    font-size: 20px !important;
    color: var(--fg) !important;
  }
  .scrub {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .t {
    font-size: 11px;
    min-width: 34px;
    text-align: center;
  }
  input[type="range"] {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 4px;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: #3a3a44;
  }
  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--fg);
  }
  .right {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }
  .vol {
    max-width: 110px;
  }
</style>
