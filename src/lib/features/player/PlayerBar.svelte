<script lang="ts">
  import * as api from "../../api";
  import AudioOutputSelector from "../audio/AudioOutputSelector.svelte";
  import { store } from "../../store.svelte";
  import { formatMs, volumeToPercent } from "../../types";
  import SpotifyConnectMenu from "./SpotifyConnectMenu.svelte";

  let {
    onOpenNowPlaying,
    onOpenArtwork,
    onOpenLyrics,
    onOpenJams,
  }: {
    onOpenNowPlaying: () => void;
    onOpenArtwork: () => void;
    onOpenLyrics: () => void;
    onOpenJams: () => void;
  } = $props();

  const pb = $derived(store.playback);
  const pct = $derived(
    pb.durationMs > 0 ? (pb.positionMs / pb.durationMs) * 100 : 0,
  );
  let seekDraft = $state<number | null>(null);
  let volumeDraft = $state<number | null>(null);
  const seekPct = $derived(seekDraft ?? pct);
  const volumePct = $derived(volumeDraft ?? volumeToPercent(pb.volume));
  const lyricsAvailable = $derived(
    store.lyrics?.status === "available" &&
      (store.lyrics.synced.length > 0 || Boolean(store.lyrics.plain)),
  );

  function previewSeek(e: Event) {
    seekDraft = Number((e.currentTarget as HTMLInputElement).value);
  }

  function previewVolume(e: Event) {
    volumeDraft = Number((e.currentTarget as HTMLInputElement).value);
  }

  async function onSeek(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    await store.run(() => api.seek(Math.round((v / 100) * pb.durationMs)));
    seekDraft = null;
  }

  async function onVolume(e: Event) {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    await store.run(() => api.setVolume(v));
    volumeDraft = null;
  }

  async function changeOutput(value: string | null) {
    await store.run(() =>
      store.saveAudioSettings(value, store.settings.equalizer),
    );
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

<div class="wrap">
  <footer>
    <div class="now">
      <button
        class="art"
        onclick={onOpenArtwork}
        disabled={!pb.track}
        title={pb.track?.albumId ? `Open ${pb.track.album}` : "Open track details"}
        aria-label={pb.track?.albumId ? `Open album ${pb.track.album}` : "Open track details"}
      >
        {#if pb.track?.coverUrl}
          <img src={pb.track.coverUrl} alt="" width="46" height="46" />
        {:else}
          <span class="ph"></span>
        {/if}
      </button>
      <button class="meta-button" onclick={onOpenNowPlaying} disabled={!pb.track} title="Open now playing">
        <span class="meta">
          <span class="name truncate">{pb.track?.name ?? "Nothing playing"}</span>
          <span class="artist muted truncate">
            {pb.track?.artists.join(", ") ?? ""}
          </span>
        </span>
      </button>
    </div>

    <div class="center">
      <div class="controls">
        <button
          class:on={pb.shuffle}
          onclick={() => store.run(() => api.setShuffle(!pb.shuffle))}
          title="Shuffle"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M16 3h5v5M4 20 21 3M21 16v5h-5M15 15l6 6M4 4l5 5" />
          </svg>
        </button>
        <button onclick={() => store.run(api.previousTrack)} title="Previous">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M6 5h2v14H6zM20 5v14L9 12z" />
          </svg>
        </button>
        <button
          class="pp"
          onclick={() => store.run(api.playPause)}
          title={pb.isPlaying ? "Pause" : "Play"}
        >
          {#if pb.isLoading}
            <span class="dots">…</span>
          {:else if pb.isPlaying}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <path d="M7 4h4v16H7zM13 4h4v16h-4z" />
            </svg>
          {:else}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <path d="M6 4l14 8-14 8z" />
            </svg>
          {/if}
        </button>
        <button onclick={() => store.run(api.nextTrack)} title="Next">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M16 5h2v14h-2zM4 5v14l11-7z" />
          </svg>
        </button>
        <button
          class:on={pb.repeatContext || pb.repeatTrack}
          onclick={cycleRepeat}
          title={pb.repeatTrack ? "Repeat track" : "Repeat"}
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <path d="M17 2l4 4-4 4" /><path d="M3 11v-1a4 4 0 0 1 4-4h14" />
            <path d="M7 22l-4-4 4-4" /><path d="M21 13v1a4 4 0 0 1-4 4H3" />
          </svg>
          {#if pb.repeatTrack}<span class="one">1</span>{/if}
        </button>
      </div>

      <div class="scrub">
        <span class="t muted">{formatMs(pb.positionMs)}</span>
        <span class="rail" class:disabled={!pb.track} style="--frac: {seekPct / 100}">
          <input
            type="range"
            min="0"
            max="100"
            step="0.1"
            value={seekPct}
            oninput={previewSeek}
            onchange={onSeek}
            onblur={() => (seekDraft = null)}
            disabled={!pb.track}
            aria-label="Seek"
          />
        </span>
        <span class="t muted">{formatMs(pb.durationMs)}</span>
      </div>
    </div>

    <div class="right">
      <button
        class="lyrics-button"
        onclick={onOpenLyrics}
        disabled={!lyricsAvailable}
        title={lyricsAvailable ? "Open fullscreen lyrics" : store.lyricsLoading ? "Lyrics are loading" : "Lyrics are unavailable"}
        aria-label="Open fullscreen lyrics"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M9 18V5l10-2v13" />
          <circle cx="6" cy="18" r="3" /><circle cx="16" cy="16" r="3" />
        </svg>
        <span>Lyrics</span>
      </button>
      <button class="jam-button" onclick={onOpenJams} title="Spotify Jam" aria-label="Open Spotify Jam">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <path d="M17 20c0-3-2-5-5-5s-5 2-5 5" /><circle cx="12" cy="8" r="4" />
        </svg>
      </button>
      <SpotifyConnectMenu />
      <AudioOutputSelector
        value={store.settings.outputDevice}
        compact
        onChange={(value) => void changeOutput(value)}
      />
      <svg class="vicon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <path d="M4 9v6h4l5 4V5L8 9z" /><path d="M17 8a5 5 0 0 1 0 8" />
      </svg>
      <span class="rail vol" style="--frac: {volumePct / 100}">
        <input
          type="range"
          min="0"
          max="100"
          value={volumePct}
          oninput={previewVolume}
          onchange={onVolume}
          onblur={() => (volumeDraft = null)}
          aria-label="Volume"
        />
      </span>
    </div>
  </footer>
</div>

<style>
  /* The bar floats with margin on three sides. Edge-to-edge kills the glass
     illusion — the background has to wrap around it to read as depth. */
  .wrap {
    padding: 0 18px 18px;
  }
  footer {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) minmax(320px, 2fr) minmax(170px, 1fr);
    align-items: center;
    gap: 22px;
    padding: 11px 18px;
    border-radius: var(--r-lg);
    background: var(--glass-raised);
    border: 1px solid var(--hairline);
    backdrop-filter: blur(36px) saturate(1.6);
    box-shadow: var(--shadow-raised), var(--edge);
  }

  .now {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    text-align: left;
  }
  .art {
    flex: none;
    padding: 0;
    border-radius: 9px;
  }
  .art:not(:disabled):hover img,
  .art:not(:disabled):focus-visible img {
    filter: brightness(1.12);
    transform: scale(1.025);
  }
  .now img,
  .ph {
    width: 46px;
    height: 46px;
    border-radius: 9px;
    object-fit: cover;
    flex: none;
    background: rgba(255, 241, 224, 0.08);
    transition: filter var(--motion-fast), transform var(--motion-fast);
  }
  .meta-button {
    min-width: 0;
    padding: 2px 0;
    text-align: left;
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .artist {
    font-size: 12px;
  }

  .center {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .controls button {
    position: relative;
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    color: var(--fg-dim);
    transition: color 0.15s, background 0.15s;
  }
  .controls button:hover {
    color: var(--fg);
    background: var(--glass-hover);
  }
  .controls button.on {
    color: var(--accent);
  }
  .one {
    position: absolute;
    right: 2px;
    bottom: 1px;
    font-size: 9px;
    font-weight: 700;
  }
  .pp {
    width: 38px;
    height: 38px;
    background: var(--fg);
    color: var(--ink);
  }
  .pp:hover {
    transform: scale(1.05);
  }
  .dots {
    font-size: 15px;
    line-height: 1;
  }

  .scrub {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
  }
  .t {
    font-size: 11px;
    min-width: 34px;
    text-align: center;
    font-variant-numeric: tabular-nums;
  }

  /* The filled portion is painted on the wrapper from --frac, so the native
     input can stay fully transparent and still handle the interaction.
     A native thumb never travels edge-to-edge: the browser keeps its center
     between half its own width and (100% - half its width), so painting the
     fill at a raw `--frac * 100%` put the fill boundary and the visible thumb
     at two different points except exactly at 0% and 100%. `--fill` mirrors
     the browser's own thumb-travel formula (using --thumb, kept equal to the
     thumb's width/height below) so the two always coincide. */
  .rail {
    --thumb: 11px;
    --fill: calc(var(--thumb) / 2 + (100% - var(--thumb)) * var(--frac));
    position: relative;
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(
      to right,
      var(--fg) 0 var(--fill),
      rgba(255, 241, 224, 0.16) var(--fill) 100%
    );
  }
  footer:hover .rail {
    background: linear-gradient(
      to right,
      var(--accent) 0 var(--fill),
      rgba(255, 241, 224, 0.16) var(--fill) 100%
    );
  }
  .rail input[type="range"] {
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 100%;
    height: 16px;
    margin: 0;
    padding: 0;
    border: 0;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    cursor: pointer;
  }

  .rail input[type="range"]::-webkit-slider-runnable-track {
    height: 4px;
    border: 0;
    background: transparent;
  }
  .rail input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: var(--thumb);
    height: var(--thumb);
    margin-top: calc((4px - var(--thumb)) / 2);
    border: 0;
    border-radius: 50%;
    background: var(--fg);
    opacity: 0;
    box-shadow: 0 0 0 0 rgba(255, 241, 224, 0.25);
    transition: opacity 0.15s, box-shadow 0.15s;
  }
  .rail:hover input[type="range"]::-webkit-slider-thumb,
  .rail input[type="range"]:active::-webkit-slider-thumb,
  .rail input[type="range"]:focus-visible::-webkit-slider-thumb {
    opacity: 1;
  }
  .rail input[type="range"]:focus-visible {
    outline: none;
  }
  .rail input[type="range"]:focus-visible::-webkit-slider-thumb {
    box-shadow: 0 0 0 4px rgba(255, 241, 224, 0.22);
  }
  .rail.disabled { opacity: .45; }
  .rail input[type="range"]:disabled { cursor: default; }
  .rail input[type="range"]:disabled::-webkit-slider-thumb { opacity: 0; }

  .right {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    color: var(--fg-dim);
    /* A grid item's default min-width is auto, which floors it at its
       content's natural size — the actual cause of the overlap: at a track
       width the responsive breakpoints below hadn't yet trimmed for, .right
       needed more room than its minmax(…, 1fr) track guaranteed and spilled
       over the center column instead of clipping. min-width:0 lets it
       actually shrink to the track; overflow:hidden makes a still-too-narrow
       track clip cleanly instead of spilling. */
    min-width: 0;
    overflow: hidden;
  }
  .lyrics-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    padding: 0 9px;
    border-radius: var(--control-radius);
    color: var(--fg-dim);
  }
  .lyrics-button:hover:not(:disabled),
  .lyrics-button:focus-visible {
    color: var(--fg);
    background: var(--glass-hover);
  }
  .jam-button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: var(--control-radius);
    color: var(--fg-dim);
  }
  .jam-button:hover,
  .jam-button:focus-visible {
    color: var(--fg);
    background: var(--glass-hover);
  }
  .vicon {
    flex: none;
  }
  .vol {
    max-width: 74px;
    flex: none;
    width: 74px;
  }
  /* Between full width and the 920px breakpoint below, .right's content
     (lyrics button, Connect menu, output selector, volume) can add up to
     more natural width than the grid's minmax(170px, 1fr) floor guarantees,
     so on a merely-narrow (not yet phone-narrow) window they overlapped the
     center transport controls instead of shrinking. Trim the two widest,
     least essential pieces — the output selector's device-name label and the
     lyrics button's text — before that happens, rather than jumping straight
     to hiding volume entirely at 920px. */
  @media (max-width: 1180px) {
    .lyrics-button span {
      display: none;
    }
    :global(.output-selector.compact .compact-label) {
      max-width: 64px;
    }
  }
  @media (max-width: 920px) {
    footer {
      grid-template-columns: minmax(150px, 1fr) minmax(280px, 1.7fr) auto;
      gap: 12px;
    }
    .vicon,
    .vol {
      display: none;
    }
  }
</style>
