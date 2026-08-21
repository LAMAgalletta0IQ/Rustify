<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import * as api from "../../api";
  import { store } from "../../store.svelte";
  import { formatMs, type QueueView } from "../../types";

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
      if (lyrics.synced[index].startMs > pb.positionMs) break;
      current = index;
    }
    return current;
  });

  let queue = $state<QueueView | null>(null);
  let queueLoading = $state(false);
  let panel = $state<"lyrics" | "queue">("lyrics");
  let lyricsPanel = $state<HTMLElement | null>(null);
  let followMode = $state<"following" | "manual">("following");
  let fullscreen = $state(false);
  let fullscreenChanging = $state(false);
  let seekDraft = $state<number | null>(null);
  let scrollFrame: number | null = null;
  let mounted = $state(false);
  let queuedTrackUri: string | null | undefined = undefined;

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
    <span>{pb.audioQualityLabel}{!pb.isActiveDevice ? " · remote quality unavailable" : ""}</span>
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
      <button
        class="artwork"
        onclick={onNavigateArtwork}
        disabled={!pb.track}
        title="Open album or track details"
      >
        {#if pb.track?.coverUrl}
          <img
            src={pb.track.coverUrl}
            alt={`Artwork for ${pb.track.name}`}
          />
        {:else}
          <span class="art-placeholder"></span>
        {/if}
      </button>
      <div class="track-copy">
        <h1>{pb.track?.name ?? "Nothing playing"}</h1>
        <p>{pb.track?.artists.join(", ") ?? ""}</p>
        <small>{pb.track?.album ?? ""}</small>
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
        {#if panel === "lyrics" && followMode === "manual"}
          <button class="resume" onclick={resumeFollowing}
            >Return to current line</button
          >
        {/if}
      </div>

      {#if panel === "lyrics"}
        <div
          class="lyrics"
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
            {:else}
              <p class="muted state">Queue is empty.</p>
            {/each}
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
  .artwork {
    align-self: center;
    width: min(100%, 53vh);
    aspect-ratio: 1;
    padding: 0;
    overflow: hidden;
    border-radius: var(--r-lg);
    box-shadow: 0 24px 70px rgba(8, 5, 2, 0.46);
    transition:
      transform var(--motion-fast),
      filter var(--motion-fast);
  }
  .artwork:hover:not(:disabled),
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
  .panel-tabs .resume {
    margin-left: auto;
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
  .lyric-line {
    display: block;
    width: 100%;
    margin: 0 0 8px;
    padding: 10px 12px;
    border-radius: 12px;
    color: rgba(245, 240, 230, 0.4);
    text-align: left;
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
    .artwork {
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
    .artwork {
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
    .lyric-line {
      transition: none;
    }
  }
</style>
