<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    currentMonitor,
    getCurrentWindow,
    PhysicalPosition,
    type PhysicalSize,
  } from "@tauri-apps/api/window";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import * as api from "../../api";
  import { store } from "../../store.svelte";
  import LyricsPane from "./LyricsPane.svelte";
  import SleepTimerBadge from "./SleepTimerBadge.svelte";
  import FullscreenChrome from "./FullscreenChrome.svelte";
  import {
    formatMs,
    type AudioCapability,
    type EpisodeResume,
    type MusicVideoCapability,
    type QueueView,
  } from "../../types";

  let {
    onClose,
    onNavigateArtwork,
    onFullscreenChange = (_: boolean) => {},
    startFullscreen = false,
  }: {
    onClose: () => void;
    onNavigateArtwork: () => void;
    onFullscreenChange?: (value: boolean) => void;
    startFullscreen?: boolean;
  } = $props();

  const appWindow = getCurrentWindow();
  const pb = $derived(store.playback);
  const lyrics = $derived(store.lyrics);
  const statusText = $derived(
    `${pb.activeDevice?.name ?? "No active device"} · ${pb.connectionStatus}${pb.isActiveDevice ? ` · ${pb.audioQualityLabel}` : " · remote quality unavailable"}`,
  );
  function rgbFromArgb(value: number) {
    return `#${((value >>> 0) & 0xffffff).toString(16).padStart(6, "0")}`;
  }

  /* Dominant colour of the current artwork, sampled client-side.
     Spotify's lyrics response carries a designed `colors.background`, but only
     for tracks that *have* lyrics — instrumentals, podcasts and anything the
     provider doesn't cover come back with none, which left fullscreen stuck on
     the same flat brown regardless of what was on screen. Sampling the cover
     covers those. Kept as a fallback rather than the primary source: where
     Spotify does ship a colour it is the better one, having been picked for
     contrast against the lyric text rather than by averaging. */
  let coverTone = $state<string | null>(null);

  /** Downscale to 12x12 and average, weighting by saturation so a mostly-grey
   * sleeve with one coloured element still reads as that colour instead of
   * mud. Near-black and near-white pixels are dropped entirely — they are
   * usually background, and including them only ever pulls the result toward
   * grey. */
  function dominantColor(image: HTMLImageElement): string | null {
    const size = 12;
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = size;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) return null;
    context.drawImage(image, 0, 0, size, size);
    let data: Uint8ClampedArray;
    try {
      data = context.getImageData(0, 0, size, size).data;
    } catch {
      // Tainted canvas: the CDN did not answer the crossOrigin request with
      // permissive CORS headers. Not an error worth surfacing — the view just
      // keeps its static background.
      return null;
    }
    let r = 0;
    let g = 0;
    let b = 0;
    let weight = 0;
    for (let index = 0; index < data.length; index += 4) {
      const [pr, pg, pb_] = [data[index], data[index + 1], data[index + 2]];
      const max = Math.max(pr, pg, pb_);
      const min = Math.min(pr, pg, pb_);
      if (max < 26 || min > 236) continue;
      // +0.12 so a genuinely monochrome sleeve still contributes something
      // rather than leaving weight at 0 and returning null.
      const saturation = (max === 0 ? 0 : (max - min) / max) + 0.12;
      r += pr * saturation;
      g += pg * saturation;
      b += pb_ * saturation;
      weight += saturation;
    }
    if (weight === 0) return null;
    const channel = (value: number) =>
      Math.max(0, Math.min(255, Math.round(value / weight)));
    return `rgb(${channel(r)}, ${channel(g)}, ${channel(b)})`;
  }

  $effect(() => {
    const url = pb.track?.coverUrl ?? null;
    coverTone = null;
    if (!url) return;
    let cancelled = false;
    const image = new Image();
    // Required for getImageData below; without it the canvas is tainted even
    // though the CDN allows the read.
    image.crossOrigin = "anonymous";
    image.onload = () => {
      if (!cancelled) coverTone = dominantColor(image);
    };
    image.src = url;
    return () => {
      cancelled = true;
      image.onload = null;
    };
  });

  /* Fullscreen only. In the windowed view this panel sits inside the ordinary
     app chrome, and tinting it per-track would fight the ambient wash rather
     than extend it. */
  const toneStyle = $derived.by(() => {
    const tone = lyrics?.colors
      ? rgbFromArgb(lyrics.colors.background)
      : coverTone;
    return tone ? `--np-tone:${tone}` : undefined;
  });

  let queue = $state<QueueView | null>(null);
  let queueLoading = $state(false);
  let episodeResume = $state<EpisodeResume | null>(null);
  let episodeGeneration = 0;
  let musicVideo = $state<MusicVideoCapability | null>(null);
  let videoGeneration = 0;
  let audioCapability = $state<AudioCapability | null>(null);
  let audioGeneration = 0;
  let panel = $state<"lyrics" | "queue">("lyrics");
  let followMode = $state<"following" | "manual">("following");
  let lyricsPaneRef: LyricsPane | undefined = $state();
  let fullscreen = $state(false);
  let fullscreenChanging = $state(false);
  let seekDraft = $state<number | null>(null);
  let mounted = $state(false);
  let queuedTrackUri: string | null | undefined = undefined;
  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    brokenImages = new Set(brokenImages).add(url);
  }

  const positionPercent = $derived(
    seekDraft ??
      (pb.durationMs > 0 ? (pb.positionMs / pb.durationMs) * 100 : 0),
  );

  async function refreshQueue() {
    queueLoading = true;
    try {
      queue = await api.getQueue();
    } catch (error) {
      store.handleError(error);
    } finally {
      queueLoading = false;
    }
  }

  $effect(() => {
    const trackUri = pb.track?.uri ?? null;
    if (trackUri === queuedTrackUri) return;
    queuedTrackUri = trackUri;
    untrack(refreshQueue);
  });

  $effect(() => {
    const uri = pb.track?.uri ?? null;
    const generation = ++episodeGeneration;
    episodeResume = null;
    if (uri?.startsWith("spotify:episode:")) {
      api
        .getEpisodeResume(uri)
        .then((value) => {
          if (generation === episodeGeneration) episodeResume = value;
        })
        .catch(() => {});
    }
  });

  $effect(() => {
    const uri = pb.track?.uri ?? null;
    const generation = ++videoGeneration;
    musicVideo = null;
    if (uri?.startsWith("spotify:track:")) {
      api
        .getMusicVideoCapability(uri)
        .then((value) => {
          if (generation === videoGeneration && value.available) musicVideo = value;
        })
        .catch(() => {});
    }
  });

  $effect(() => {
    const uri = pb.track?.uri ?? null;
    const generation = ++audioGeneration;
    audioCapability = null;
    if (uri?.startsWith("spotify:track:")) {
      api
        .getAudioCapability(uri)
        .then((value) => {
          if (generation === audioGeneration) audioCapability = value;
        })
        .catch(() => {});
    }
  });

  async function toggleEpisodeCompleted() {
    const episode = episodeResume;
    if (!episode) return;
    const completed = !episode.completed;
    await store.run(() => api.setEpisodeCompleted(episode.uri, completed));
    episodeResume = {
      ...episode,
      completed,
      positionMs: completed ? 0 : episode.positionMs,
      hasState: true,
    };
  }

  // Deliberately not `appWindow.setFullscreen()`. On Windows, a real OS
  // fullscreen transition on a `decorations:false, transparent:true` window
  // makes Windows treat it as an exclusive-fullscreen surface: the frame
  // Windows re-adds mid-transition is the "classic Windows 7 titlebar" flash,
  // Acrylic gets re-composited (and flickers) whenever focus moves to another
  // app while this window is fullscreen on another monitor, and system
  // flyouts (volume/media OSD) reposition themselves to the top of the screen
  // to avoid "covering" the fullscreen content. None of that is triggered by
  // just resizing an ordinary window to cover the monitor — so "fullscreen"
  // here means moving/resizing to the current monitor's bounds and letting
  // the CSS below hide the chrome, never touching real OS fullscreen state.
  let savedBounds: { position: PhysicalPosition; size: PhysicalSize } | null =
    null;
  let savedResizable = true;

  async function enterFullscreenBounds() {
    const monitor = await currentMonitor();
    if (!monitor) return false;
    savedBounds = {
      position: await appWindow.outerPosition(),
      size: await appWindow.outerSize(),
    };
    savedResizable = await appWindow.isResizable();
    await appWindow.setResizable(false);
    await appWindow.setPosition(monitor.position);
    await appWindow.setSize(monitor.size);

    // setSize() above already sets the window's *inner* (content/visible)
    // size directly — per its own doc comment, "resizes the window with a
    // new inner size" — so the visible area is already exactly monitor.size,
    // no correction needed there. (An earlier version of this also grew the
    // size by the position correction below, which actually made the inner
    // content *bigger* than the monitor and spilled it onto the next one —
    // setSize was never the part that needed fixing.)
    //
    // setPosition() is different: its own doc comment says it "sets the
    // window *outer* position", i.e. the whole window rect including
    // whatever invisible resize-border margin Windows still draws around a
    // borderless window even with resizing disabled. That margin sits between
    // the outer rect and the visible content, so positioning the outer rect
    // at the monitor's origin left the *visible* content a few pixels short
    // of the monitor's actual left/top edge — the sliver-of-desktop bug.
    // innerPosition() reports where the content actually ended up; measure
    // the gap against outerPosition() and shift the outer position by
    // exactly that much, whatever the margin turns out to be on this
    // machine/monitor/DPI.
    const outerPos = await appWindow.outerPosition();
    const innerPos = await appWindow.innerPosition();
    const dx = innerPos.x - outerPos.x;
    const dy = innerPos.y - outerPos.y;
    if (dx || dy) {
      await appWindow.setPosition(
        new PhysicalPosition(monitor.position.x - dx, monitor.position.y - dy),
      );
    }
    return true;
  }

  async function exitFullscreenBounds() {
    if (savedBounds) {
      await appWindow.setPosition(savedBounds.position);
      await appWindow.setSize(savedBounds.size);
      savedBounds = null;
    }
    await appWindow.setResizable(savedResizable);
  }

  async function setFullscreen(value: boolean) {
    if (fullscreenChanging || fullscreen === value) return;
    fullscreenChanging = true;
    try {
      if (value) {
        if (!(await enterFullscreenBounds())) return;
      } else {
        await exitFullscreenBounds();
      }
      fullscreen = value;
      onFullscreenChange(value);
      if (value) panel = "lyrics";
    } catch (error) {
      store.handleError(error);
    } finally {
      fullscreenChanging = false;
    }
  }

  async function closeView() {
    if (fullscreen) await setFullscreen(false);
    onClose();
  }

  async function seekPercent(event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    await store.run(() =>
      api.seek(Math.round((value / 100) * pb.durationMs)),
    );
    seekDraft = null;
  }

  $effect(() => {
    if (mounted && startFullscreen && !fullscreen && !fullscreenChanging) {
      void setFullscreen(true);
    }
  });

  onMount(() => {
    mounted = true;
    // No `appWindow.isFullscreen()` sync needed any more — "fullscreen" here
    // is this component's own bounds bookkeeping (see setFullscreen above),
    // not OS window state, so a fresh mount always starts non-fullscreen and
    // the $effect above drives entry when `startFullscreen` is set.
    // The arrow/page-key-while-focused-in-lyrics branch lives in
    // LyricsPane.svelte now — it needs that panel's own DOM ref.
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "F11") {
        event.preventDefault();
        void setFullscreen(!fullscreen);
      } else if (event.key === "Escape" && fullscreen) {
        event.preventDefault();
        void setFullscreen(false);
      }
    };
    window.addEventListener("keydown", keydown);
    return () => {
      mounted = false;
      window.removeEventListener("keydown", keydown);
      if (fullscreen) void exitFullscreenBounds();
      onFullscreenChange(false);
    };
  });

  // Queue mutations (Jam/DJ additions, remote Connect changes, autoplay
  // refills) happen server-side; without this the panel only refreshes on
  // track change and shows a stale list until the next track boundary.
  onMount(() => {
    let disposed = false;
    let stop: UnlistenFn | undefined;
    listen<QueueView>(api.EVENT_QUEUE, (event) => {
      queue = event.payload;
      queueLoading = false;
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop?.();
    };
  });
</script>

<div
  class="now-playing"
  class:fullscreen
  class:toned={fullscreen && toneStyle !== undefined}
  style={toneStyle}
>
  <FullscreenChrome
    {fullscreen}
    {fullscreenChanging}
    {statusText}
    onEnterFullscreen={() => void setFullscreen(true)}
    onExitFullscreen={() => void setFullscreen(false)}
    onClose={() => void closeView()}
  />

  <main>
    <section class="track-pane">
      <div class="artwork-wrap" role="group" aria-label="Artwork and playback controls">
        <button
          class="artwork"
          onclick={onNavigateArtwork}
          disabled={!pb.track}
          title="Open album or track details"
        >
          {#if pb.track?.coverUrl && !brokenImages.has(pb.track.coverUrl)}
            <img
              src={pb.track.coverUrl}
              alt={`Artwork for ${pb.track.name}`}
              onerror={() => onArtworkError(pb.track?.coverUrl)}
            />
          {:else}
            <span class="art-placeholder"></span>
          {/if}
        </button>
        <div class="artwork-overlay">
          <button
            onclick={(event) => {
              event.stopPropagation();
              void store.run(api.previousTrack);
            }}
            disabled={!pb.track}
            title="Previous"
            aria-label="Previous track"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M6 5h2v14H6zM20 5v14L9 12z" /></svg>
          </button>
          <button
            class="play"
            onclick={(event) => {
              event.stopPropagation();
              void store.run(api.playPause);
            }}
            disabled={!pb.track}
            title={pb.isPlaying ? "Pause" : "Play"}
            aria-label={pb.isPlaying ? "Pause" : "Play"}
          >
            {#if pb.isLoading}
              <span class="dots">…</span>
            {:else if pb.isPlaying}
              <svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor"><path d="M7 4h4v16H7zM13 4h4v16h-4z" /></svg>
            {:else}
              <svg width="17" height="17" viewBox="0 0 24 24" fill="currentColor"><path d="M6 4l14 8-14 8z" /></svg>
            {/if}
          </button>
          <button
            onclick={(event) => {
              event.stopPropagation();
              void store.run(api.nextTrack);
            }}
            disabled={!pb.track}
            title="Next"
            aria-label="Next track"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M16 5h2v14h-2zM4 5v14l11-7z" /></svg>
          </button>
        </div>
      </div>
      <div class="track-copy">
        <h1>{pb.track?.name ?? "Nothing playing"}</h1>
        <p>{pb.track?.artists.join(", ") ?? ""}</p>
        <small>{pb.track?.album ?? ""}</small>
        {#if audioCapability?.losslessMetadataAvailable}
          <div class="lossless-state" title={audioCapability.losslessBlocker ?? undefined}>
            <span
              >Spotify Lossless metadata · {audioCapability.losslessDecoderAvailable
                ? "FLAC decoder ready"
                : "unsupported FLAC variant"} · protected key unavailable</span
            >
          </div>
        {/if}
        {#if episodeResume}
          <div class="episode-state">
            <span
              >{episodeResume.completed
                ? "Played"
                : episodeResume.positionMs > 0
                  ? `Spotify resume · ${formatMs(episodeResume.positionMs)}`
                  : "Podcast episode"}</span
            >
            <button class:on={episodeResume.completed} onclick={toggleEpisodeCompleted}
              >{episodeResume.completed ? "Mark unplayed" : "Mark played"}</button
            >
          </div>
        {/if}
        {#if musicVideo}
          <div class="video-state" title={musicVideo.playbackBlocker ?? undefined}>
            {#if musicVideo.images[0] && !brokenImages.has(musicVideo.images[0].url)}
              <img
                src={musicVideo.images[0].url}
                alt="Music video thumbnail"
                onerror={() => onArtworkError(musicVideo?.images[0]?.url)}
              />
            {/if}
            <span>Spotify music video available · protected playback</span>
            {#if musicVideo.openUrl}
              <button onclick={() => openUrl(musicVideo!.openUrl!)}>Watch in Spotify</button>
            {/if}
          </div>
        {/if}
      </div>

      {#if fullscreen}
        <div class="timeline">
          <span>{formatMs(pb.positionMs)}</span>
          <input
            type="range"
            min="0"
            max="100"
            step="0.1"
            value={positionPercent}
            oninput={(event) =>
              (seekDraft = Number(event.currentTarget.value))}
            onchange={seekPercent}
            onblur={() => (seekDraft = null)}
            disabled={!pb.track}
            aria-label="Song position"
          />
          <span>{formatMs(pb.durationMs)}</span>
        </div>
      {/if}
    </section>

    <section class="content-pane">
      <div class="panel-tabs" role="tablist" aria-label="Now playing details">
        <button
          role="tab"
          aria-selected={panel === "lyrics"}
          class:on={panel === "lyrics"}
          onclick={() => (panel = "lyrics")}>Lyrics</button
        >
        <button
          role="tab"
          aria-selected={panel === "queue"}
          class:on={panel === "queue"}
          onclick={() => (panel = "queue")}>Queue</button
        >
        <SleepTimerBadge />
        {#if panel === "lyrics" && lyrics?.provider}
          <span class="provider">{lyrics.provider}{lyrics.language ? ` · ${lyrics.language}` : ""}</span>
        {/if}
        {#if panel === "lyrics" && followMode === "manual"}
          <button class="resume" onclick={() => lyricsPaneRef?.resumeFollowing()}
            >Return to current line</button
          >
        {/if}
      </div>

      {#if panel === "lyrics"}
        <LyricsPane bind:this={lyricsPaneRef} bind:followMode />
      {:else}
        <div class="queue" role="tabpanel" aria-label="Playback queue">
          {#if queueLoading && !queue}
            <p class="muted state">Loading queue…</p>
          {:else if queue}
            {#if queue.previous.length}
              <p class="queue-label">Previously played</p>
              {#each queue.previous.slice(-3) as track, index (track.uri + index)}
                <div class="past">
                  <strong class="truncate">{track.name}</strong>
                  <span class="truncate">{track.artists.join(", ")}</span>
                </div>
              {/each}
            {/if}
            {#if queue.queue.length}
              <p class="queue-label">Next in queue</p>
              {#each queue.queue as track, index (track.uri + index)}
                <button
                  onclick={() =>
                    store.run(() =>
                      api.loadTracks(
                        queue!.queue.map((item) => item.uri),
                        track.uri,
                      ),
                    )}
                >
                  <strong class="truncate">{track.name}</strong>
                  <span class="truncate"
                    >{track.artists.join(", ")} · {formatMs(
                      track.durationMs,
                    )}</span
                  >
                </button>
              {/each}
            {/if}
            {#if queue.autoplay.length}
              <p class="queue-label">Autoplay</p>
              {#each queue.autoplay as track, index (track.uri + index)}
                <button
                  onclick={() =>
                    store.run(() =>
                      api.loadTracks(
                        queue!.autoplay.map((item) => item.uri),
                        track.uri,
                      ),
                    )}
                >
                  <strong class="truncate">{track.name}</strong>
                  <span class="truncate"
                    >{track.artists.join(", ")} · {formatMs(
                      track.durationMs,
                    )}</span
                  >
                </button>
              {/each}
            {/if}
            {#if !queue.queue.length && !queue.autoplay.length && !queue.previous.length}
              <p class="muted state">Queue is empty.</p>
            {/if}
          {/if}
        </div>
      {/if}
    </section>
  </main>
</div>

<style>
  .now-playing {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: rgba(17, 11, 6, 0.34);
    transition: background var(--motion-slow) var(--ease-standard);
  }
  /* Artwork-derived wash, fullscreen only. Layered over the same base tint
     rather than replacing it, and kept well under half strength: --np-tone is
     an unmodified colour straight off a sleeve, so at full weight it is
     routinely brighter than the white lyric text sitting on top of it. The
     radial puts the strongest point behind the artwork at the top, which is
     where the eye reads the match. */
  .now-playing.toned {
    background:
      radial-gradient(
        120% 82% at 50% 0%,
        color-mix(in srgb, var(--np-tone) 40%, transparent),
        transparent 72%
      ),
      linear-gradient(
        180deg,
        color-mix(in srgb, var(--np-tone) 14%, transparent),
        rgba(14, 9, 4, 0.52)
      ),
      rgba(17, 11, 6, 0.34);
  }
  main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(260px, 0.86fr) minmax(360px, 1.2fr);
    gap: clamp(24px, 5vw, 72px);
    padding: clamp(16px, 4vh, 48px) clamp(24px, 6vw, 86px);
    overflow: hidden;
  }
  .track-pane {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }
  .artwork-wrap {
    position: relative;
    align-self: center;
    width: min(100%, 53vh);
    aspect-ratio: 1;
  }
  .artwork {
    width: 100%;
    height: 100%;
    padding: 0;
    overflow: hidden;
    border-radius: var(--r-lg);
    box-shadow: 0 24px 70px rgba(8, 5, 2, 0.46);
    transition:
      transform var(--motion-fast),
      filter var(--motion-fast);
  }
  .artwork-wrap:hover .artwork:not(:disabled),
  .artwork:focus-visible {
    transform: translateY(-2px);
    filter: brightness(1.08);
  }
  .artwork img,
  .art-placeholder {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
  }
  .art-placeholder {
    background: rgba(255, 241, 224, 0.07);
  }
  .artwork-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 14px;
    border-radius: var(--r-lg);
    background: rgba(13, 9, 5, 0.5);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--motion-fast);
  }
  .artwork-wrap:hover .artwork-overlay,
  .artwork-wrap:focus-within .artwork-overlay {
    opacity: 1;
    pointer-events: auto;
  }
  .artwork-overlay button {
    display: grid;
    place-items: center;
    flex: none;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    color: var(--fg);
    background: rgba(15, 10, 6, 0.6);
    border: 1px solid rgba(255, 255, 255, 0.16);
    transition: color var(--motion-fast), transform var(--motion-fast);
  }
  .artwork-overlay button:hover:not(:disabled) {
    color: var(--accent);
    transform: scale(1.06);
  }
  .artwork-overlay button:disabled {
    opacity: 0.4;
  }
  .artwork-overlay .play {
    width: 54px;
    height: 54px;
    color: var(--ink);
    background: var(--fg);
  }
  .artwork-overlay .play:hover:not(:disabled) {
    color: var(--ink);
  }
  .artwork-overlay .dots {
    font-size: 18px;
    line-height: 1;
  }
  .track-copy {
    margin-top: 20px;
  }
  .track-copy h1 {
    margin: 0;
    font-size: clamp(22px, 3vw, 38px);
    line-height: 1.12;
  }
  .track-copy p {
    margin: 7px 0 2px;
    color: var(--fg-dim);
    font-size: 15px;
  }
  .track-copy small {
    color: var(--fg-faint);
  }
  .episode-state,
  .video-state,
  .lossless-state {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 8px;
    margin-top: 9px;
    color: var(--fg-dim);
    font-size: 11px;
  }
  .lossless-state {
    color: var(--accent);
  }
  .episode-state button,
  .video-state button {
    padding: 5px 9px;
    border: 1px solid var(--control-border);
  }
  .episode-state button.on {
    color: var(--accent);
  }
  .video-state img {
    width: 44px;
    height: 25px;
    object-fit: cover;
    border-radius: 4px;
  }
  .timeline {
    display: grid;
    grid-template-columns: 42px 1fr 42px;
    align-items: center;
    gap: 10px;
    margin-top: 22px;
    color: var(--fg-dim);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .timeline input {
    width: 100%;
    accent-color: var(--accent);
  }
  .content-pane {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .panel-tabs {
    flex: none;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .panel-tabs button {
    padding: 7px 12px;
    border-radius: var(--control-radius);
    color: var(--fg-dim);
  }
  .panel-tabs button.on {
    color: var(--fg);
    background: var(--glass-strong);
  }
  .panel-tabs .provider {
    margin-left: auto;
    color: var(--fg-dim);
    font-size: 10px;
  }
  .panel-tabs .resume {
    color: var(--accent);
    border: 1px solid var(--control-border);
  }
  .queue {
    flex: 1;
    min-height: 0;
    margin-top: 8px;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
    padding: 4px;
  }
  .queue button {
    width: 100%;
    display: flex;
    flex-direction: column;
    padding: 10px;
    border-radius: var(--control-radius);
    text-align: left;
  }
  .queue button:hover,
  .queue button:focus-visible {
    background: var(--glass-hover);
  }
  .queue span {
    color: var(--fg-dim);
    font-size: 12px;
  }
  .queue .past {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 10px;
    opacity: 0.55;
  }
  .queue-label {
    margin: 14px 10px 5px;
    color: var(--fg-dim);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .state {
    padding: 12px;
  }
  @media (max-width: 800px) {
    main {
      grid-template-columns: 1fr;
      gap: 18px;
      padding: 14px 20px;
      overflow-y: auto;
    }
    .artwork-wrap {
      width: min(64vw, 36vh);
    }
    .track-copy {
      margin-top: 12px;
    }
    .content-pane {
      min-height: 52vh;
    }
  }
  @media (max-height: 590px) and (min-width: 801px) {
    main {
      padding-block: 10px;
    }
    .artwork-wrap {
      width: min(36vw, 47vh);
    }
    .track-copy {
      margin-top: 10px;
    }
    .track-copy h1 {
      font-size: 21px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .artwork,
    .artwork-overlay,
    .artwork-overlay button {
      transition: none;
    }
  }
</style>
