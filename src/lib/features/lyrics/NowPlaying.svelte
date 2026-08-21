<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import * as api from "../../api";
  import { store } from "../../store.svelte";
  import SelectMenu, { type SelectOption } from "../../ui/SelectMenu.svelte";
  import {
    formatMs,
    type AudioCapability,
    type EpisodeResume,
    type MusicVideoCapability,
    type QueueView,
    type SleepTimerStatus,
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
  const activeLine = $derived.by(() => {
    if (!lyrics?.synced.length) return -1;
    let current = -1;
    for (let index = 0; index < lyrics.synced.length; index += 1) {
      const line = lyrics.synced[index];
      if (line.startMs > pb.positionMs) break;
      // A null endMs runs to the next line's start; otherwise a vocal pause
      // (endMs already passed) leaves no line marked active rather than
      // holding the previous one highlighted through the silence.
      current = line.endMs === null || pb.positionMs < line.endMs ? index : -1;
    }
    return current;
  });
  function rgbFromArgb(value: number) {
    return `#${((value >>> 0) & 0xffffff).toString(16).padStart(6, "0")}`;
  }
  const lyricsStyle = $derived(
    lyrics?.colors
      ? `--lyrics-bg:${rgbFromArgb(lyrics.colors.background)};--lyrics-text:${rgbFromArgb(lyrics.colors.text)};--lyrics-highlight:${rgbFromArgb(lyrics.colors.highlightText)}`
      : undefined,
  );

  let queue = $state<QueueView | null>(null);
  let queueLoading = $state(false);
  let sleepTimer = $state<SleepTimerStatus | null>(null);
  let sleepClockMs = $state(Date.now());
  let episodeResume = $state<EpisodeResume | null>(null);
  let episodeGeneration = 0;
  let musicVideo = $state<MusicVideoCapability | null>(null);
  let videoGeneration = 0;
  let audioCapability = $state<AudioCapability | null>(null);
  let audioGeneration = 0;
  const sleepRemainingSeconds = $derived(
    sleepTimer?.mode === "duration" && sleepTimer.endsAtUnixMs !== null
      ? Math.max(0, Math.ceil((sleepTimer.endsAtUnixMs - sleepClockMs) / 1000))
      : (sleepTimer?.remainingSeconds ?? null),
  );
  let panel = $state<"lyrics" | "queue">("lyrics");
  let lyricsPanel = $state<HTMLElement | null>(null);
  let followMode = $state<"following" | "manual">("following");
  let fullscreen = $state(false);
  let fullscreenChanging = $state(false);
  let seekDraft = $state<number | null>(null);
  let scrollFrame: number | null = null;
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
    pb.track?.uri;
    followMode = "following";
    if (lyricsPanel) lyricsPanel.scrollTop = 0;
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

  function scrollToActive(behavior: ScrollBehavior) {
    if (!lyricsPanel || activeLine < 0) return;
    if (scrollFrame !== null) cancelAnimationFrame(scrollFrame);
    scrollFrame = requestAnimationFrame(() => {
      scrollFrame = null;
      const line = lyricsPanel?.querySelector<HTMLElement>(
        `[data-line-index="${activeLine}"]`,
      );
      if (!line || !lyricsPanel) return;
      const top = Math.max(
        0,
        line.offsetTop - (lyricsPanel.clientHeight - line.offsetHeight) / 2,
      );
      lyricsPanel.scrollTo({ top, behavior });
    });
  }

  $effect(() => {
    activeLine;
    if (followMode !== "following") return;
    scrollToActive(store.settings.reduceMotion ? "auto" : "smooth");
  });

  function observeLyricsPanel(node: HTMLElement) {
    const suspend = () => {
      followMode = "manual";
      if (scrollFrame !== null) {
        cancelAnimationFrame(scrollFrame);
        scrollFrame = null;
      }
      node.scrollTo({ top: node.scrollTop, behavior: "auto" });
    };
    const resize = new ResizeObserver(() => {
      if (followMode === "following") scrollToActive("auto");
    });
    node.addEventListener("wheel", suspend, { passive: true });
    node.addEventListener("touchmove", suspend, { passive: true });
    resize.observe(node);
    return {
      destroy() {
        node.removeEventListener("wheel", suspend);
        node.removeEventListener("touchmove", suspend);
        resize.disconnect();
      },
    };
  }

  function resumeFollowing() {
    followMode = "following";
    scrollToActive(store.settings.reduceMotion ? "auto" : "smooth");
  }

  async function seekLine(startMs: number) {
    followMode = "following";
    await store.run(() => api.seek(startMs));
  }

  async function setFullscreen(value: boolean) {
    if (fullscreenChanging || fullscreen === value) return;
    fullscreenChanging = true;
    try {
      await appWindow.setFullscreen(value);
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

  async function chooseSleep(value: string | null) {
    if (!value) return;
    await store.run(async () => {
      sleepTimer =
        value === "end"
          ? await api.sleepAtEndOfTrack()
          : value === "cancel"
            ? await api.cancelSleepTimer()
            : await api.startSleepTimer(Number(value) * 60);
    });
  }

  const sleepTimerLabel = $derived(
    sleepTimer?.active
      ? sleepTimer.mode === "endOfTrack"
        ? "Sleep · end of track"
        : `Sleep · ${Math.floor((sleepRemainingSeconds ?? 0) / 60)}:${String((sleepRemainingSeconds ?? 0) % 60).padStart(2, "0")}`
      : "Sleep timer"
  );
  const sleepTimerOptions = $derived<SelectOption[]>([
    { value: "15", label: "15 minutes" },
    { value: "30", label: "30 minutes" },
    { value: "60", label: "1 hour" },
    { value: "end", label: "End of track" },
    ...(sleepTimer?.active
      ? [{ value: "cancel", label: "Cancel timer", tone: "warning" as const }]
      : []),
  ]);

  $effect(() => {
    if (mounted && startFullscreen && !fullscreen && !fullscreenChanging) {
      void setFullscreen(true);
    }
  });

  onMount(() => {
    let disposed = false;
    mounted = true;
    void appWindow.isFullscreen().then(async (value) => {
      if (disposed) return;
      fullscreen = value;
      onFullscreenChange(value);
      if (startFullscreen && !value) await setFullscreen(true);
    });
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "F11") {
        event.preventDefault();
        void setFullscreen(!fullscreen);
      } else if (event.key === "Escape" && fullscreen) {
        event.preventDefault();
        void setFullscreen(false);
      } else if (
        lyricsPanel?.contains(document.activeElement) &&
        ["PageUp", "PageDown", "ArrowUp", "ArrowDown", "Home", "End"].includes(
          event.key,
        )
      ) {
        followMode = "manual";
      }
    };
    window.addEventListener("keydown", keydown);
    return () => {
      disposed = true;
      mounted = false;
      window.removeEventListener("keydown", keydown);
      if (scrollFrame !== null) cancelAnimationFrame(scrollFrame);
      if (fullscreen) void appWindow.setFullscreen(false);
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

  onMount(() => {
    let disposed = false;
    let stop: UnlistenFn | undefined;
    api.getSleepTimer().then((value) => {
      if (!disposed) sleepTimer = value;
    });
    listen<SleepTimerStatus>(api.EVENT_SLEEP_TIMER, (event) => {
      sleepTimer = event.payload;
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop?.();
    };
  });

  onMount(() => {
    const timer = window.setInterval(() => (sleepClockMs = Date.now()), 1000);
    return () => clearInterval(timer);
  });
</script>

<div class="now-playing" class:fullscreen>
  <header class="topbar" data-tauri-drag-region>
    {#if fullscreen}
      <button
        class="exit"
        onclick={() => void setFullscreen(false)}
        disabled={fullscreenChanging}
        title="Exit fullscreen (Esc)"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 3v5H3M16 3v5h5M8 21v-5H3M16 21v-5h5" /></svg>
        Exit fullscreen
      </button>
    {:else}
      <button
        class="exit"
        onclick={() => void closeView()}
        title="Close now playing"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6 6 18M6 6l12 12" /></svg>
        Close
      </button>
    {/if}
    <div class="drag" data-tauri-drag-region></div>
    <span>{pb.activeDevice?.name ?? "No active device"} · {pb.connectionStatus}{pb.isActiveDevice ? ` · ${pb.audioQualityLabel}` : " · remote quality unavailable"}</span>
    {#if !fullscreen}
      <button
        class="fullscreen-button"
        onclick={() => void setFullscreen(true)}
        disabled={fullscreenChanging}
        title="Open fullscreen lyrics (F11)"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 3H3v5M16 3h5v5M8 21H3v-5M16 21h5v-5" /></svg>
        Fullscreen lyrics
      </button>
    {/if}
  </header>

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
        <SelectMenu
          value={null}
          options={sleepTimerOptions}
          triggerLabel={sleepTimerLabel}
          label="Sleep timer"
          compact
          onChange={chooseSleep}
        >
          {#snippet icon()}
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="13" r="8" /><path d="M12 9v4l3 2" /><path d="M9 2h6" />
            </svg>
          {/snippet}
        </SelectMenu>
        {#if panel === "lyrics" && lyrics?.provider}
          <span class="provider">{lyrics.provider}{lyrics.language ? ` · ${lyrics.language}` : ""}</span>
        {/if}
        {#if panel === "lyrics" && followMode === "manual"}
          <button class="resume" onclick={resumeFollowing}
            >Return to current line</button
          >
        {/if}
      </div>

      {#if panel === "lyrics"}
        <div
          class="lyrics"
          class:spotify-colors={Boolean(lyrics?.colors)}
          style={lyricsStyle}
          dir={lyrics?.isRtl ? "rtl" : "ltr"}
          bind:this={lyricsPanel}
          use:observeLyricsPanel
          role="region"
          aria-label="Synchronized lyrics"
        >
          {#if !pb.track}
            <p class="muted state">Start a track to see lyrics.</p>
          {:else if store.lyricsLoading}
            <p class="muted state">Loading lyrics…</p>
          {:else if store.lyricsError}
            <p class="muted state">{store.lyricsError}</p>
          {:else if lyrics?.status === "instrumental"}
            <p class="muted state">This track is marked as instrumental.</p>
          {:else if lyrics?.status === "unavailable" || !lyrics}
            <p class="muted state">Lyrics are not available for this track.</p>
          {:else if lyrics.synced.length}
            {#each lyrics.synced as line, index (`${line.startMs}-${index}`)}
              <button
                class="lyric-line"
                class:past={index < activeLine}
                class:active={index === activeLine}
                data-line-index={index}
                onclick={() => void seekLine(line.startMs)}
                aria-label={`Seek to ${formatMs(line.startMs)}: ${line.text || "instrumental"}`}
                aria-current={index === activeLine ? "true" : undefined}
                >{line.text || "♪"}</button
              >
            {/each}
          {:else if lyrics.plain}
            {#each lyrics.plain.split("\n") as line, index (index)}
              <p class="plain-line">{line || " "}</p>
            {/each}
          {/if}
        </div>
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
  }
  .topbar {
    flex: none;
    min-height: 44px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    color: var(--fg-dim);
    font-size: 11px;
  }
  .drag {
    flex: 1;
    align-self: stretch;
  }
  .topbar button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 32px;
    padding: 0 10px;
    border-radius: var(--control-radius);
  }
  .topbar button:hover:not(:disabled) {
    color: var(--fg);
    background: var(--glass-hover);
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
  .lyrics,
  .queue {
    flex: 1;
    min-height: 0;
    margin-top: 8px;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
  }
  .lyrics {
    position: relative;
    padding: 34vh 9px;
    scroll-behavior: auto;
    mask-image: linear-gradient(
      transparent,
      #000 9%,
      #000 91%,
      transparent
    );
  }
  .lyrics.spotify-colors {
    background: radial-gradient(
      circle at 50% 35%,
      color-mix(in srgb, var(--lyrics-bg) 44%, transparent),
      transparent 62%
    );
    border-radius: var(--r-md);
  }
  .lyrics.spotify-colors .lyric-line {
    color: color-mix(in srgb, var(--lyrics-text) 48%, transparent);
  }
  .lyrics.spotify-colors .lyric-line.past {
    color: color-mix(in srgb, var(--lyrics-text) 72%, transparent);
  }
  .lyrics.spotify-colors .lyric-line.active {
    color: var(--lyrics-highlight);
  }
  .lyric-line {
    display: block;
    width: 100%;
    margin: 0 0 8px;
    padding: 10px 12px;
    border-radius: 12px;
    color: rgba(245, 240, 230, 0.4);
    text-align: start;
    font-size: clamp(20px, 3vw, 38px);
    font-weight: 650;
    line-height: 1.25;
    transition:
      color var(--motion-fast),
      background var(--motion-fast),
      transform var(--motion-fast);
  }
  .lyric-line:hover,
  .lyric-line:focus-visible {
    color: rgba(245, 240, 230, 0.86);
    background: rgba(255, 241, 224, 0.075);
  }
  .lyric-line.past {
    color: rgba(245, 240, 230, 0.57);
  }
  .lyric-line.active {
    color: var(--fg);
    background: rgba(255, 241, 224, 0.09);
    transform: translateX(5px);
    text-shadow: 0 0 22px rgba(245, 240, 230, 0.15);
  }
  .plain-line {
    margin: 0 0 15px;
    font-size: 18px;
    line-height: 1.5;
  }
  .queue {
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
    .lyrics {
      padding-block: 22vh;
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
    .artwork-overlay button,
    .lyric-line {
      transition: none;
    }
  }
</style>
