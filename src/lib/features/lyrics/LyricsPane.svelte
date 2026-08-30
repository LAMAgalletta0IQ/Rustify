<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "../../api";
  import { store } from "../../store.svelte";
  import { formatMs } from "../../types";

  let {
    followMode = $bindable<"following" | "manual">("following"),
  }: {
    followMode?: "following" | "manual";
  } = $props();

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

  let lyricsPanel = $state<HTMLElement | null>(null);
  let scrollFrame: number | null = null;

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
    pb.track?.uri;
    followMode = "following";
    if (lyricsPanel) lyricsPanel.scrollTop = 0;
  });

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

  /** Called by the parent's "Return to current line" button, which lives in
   * the shared tab bar rather than in this panel. */
  export function resumeFollowing() {
    followMode = "following";
    scrollToActive(store.settings.reduceMotion ? "auto" : "smooth");
  }

  async function seekLine(startMs: number) {
    followMode = "following";
    await store.run(() => api.seek(startMs));
  }

  // Split out of NowPlaying's single global keydown handler: this branch
  // needs `lyricsPanel`, which is private to this component. F11/Escape stay
  // in NowPlaying since they drive fullscreen, not this panel.
  onMount(() => {
    const keydown = (event: KeyboardEvent) => {
      if (
        lyricsPanel?.contains(document.activeElement) &&
        ["PageUp", "PageDown", "ArrowUp", "ArrowDown", "Home", "End"].includes(
          event.key,
        )
      ) {
        followMode = "manual";
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  });
</script>

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
      <p class="plain-line">{line || " "}</p>
    {/each}
  {/if}
</div>

<style>
  .lyrics {
    flex: 1;
    min-height: 0;
    margin-top: 8px;
    position: relative;
    padding: 34vh 9px;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
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
  .state {
    padding: 12px;
  }
  @media (max-width: 800px) {
    .lyrics {
      padding-block: 22vh;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .lyric-line {
      transition: none;
    }
  }
</style>
