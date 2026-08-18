<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "../api";
  import { store } from "../store.svelte";
  import {
    formatMs,
    type LyricsResult,
    type QueueView,
  } from "../types";

  let { onClose }: { onClose: () => void } = $props();

  let queue = $state<QueueView | null>(null);
  let loading = $state(false);
  let lyrics = $state<LyricsResult | null>(null);
  let lyricsLoading = $state(false);
  let lyricsError = $state<string | null>(null);
  let lyricsPanel: HTMLElement | null = $state(null);

  const pb = $derived(store.playback);
  const activeLine = $derived.by(() => {
    if (!lyrics?.synced.length) return -1;
    let active = -1;
    for (let i = 0; i < lyrics.synced.length; i++) {
      if (lyrics.synced[i].startMs > pb.positionMs) break;
      active = i;
    }
    return active;
  });

  async function refresh() {
    loading = true;
    try {
      queue = await api.getQueue();
    } catch (e) {
      store.handleError(e);
    } finally {
      loading = false;
    }
  }

  // Refresh the queue whenever the track changes.
  $effect(() => {
    pb.track?.uri;
    untrack(refresh);
  });

  $effect(() => {
    const track = pb.track;
    let cancelled = false;
    lyrics = null;
    lyricsError = null;
    lyricsLoading = Boolean(track);
    if (track) {
      api
        .getLyrics(
          track.name,
          track.artists[0] ?? "",
          track.album,
          track.durationMs,
        )
        .then((value) => {
          if (!cancelled) lyrics = value;
        })
        .catch((e) => {
          if (!cancelled) lyricsError = store.handleError(e, false).message;
        })
        .finally(() => {
          if (!cancelled) lyricsLoading = false;
        });
    }
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    activeLine;
    if (!lyricsPanel || activeLine < 0) return;
    const frame = requestAnimationFrame(() => {
      lyricsPanel
        ?.querySelector(".line.active")
        ?.scrollIntoView({ block: "center", behavior: store.settings.reduceMotion ? "auto" : "smooth" });
    });
    return () => cancelAnimationFrame(frame);
  });
</script>

<div class="np">
  <button class="close" onclick={onClose} title="Close">▾</button>

  <div class="art">
    {#if pb.track?.coverUrl}
      <img src={pb.track.coverUrl} alt="" />
    {:else}
      <div class="ph"></div>
    {/if}
    <h2 class="truncate">{pb.track?.name ?? "Nothing playing"}</h2>
    <p class="muted truncate">{pb.track?.artists.join(", ") ?? ""}</p>
    <p class="muted small truncate">{pb.track?.album ?? ""}</p>

  </div>

  <section class="lyrics" aria-labelledby="lyrics-heading">
    <header><h3 id="lyrics-heading">Lyrics</h3>{#if lyrics}<span class="provider">{lyrics.provider}</span>{/if}</header>
    <div class="lyrics-scroll" bind:this={lyricsPanel} aria-live="polite">
      {#if !pb.track}
        <p class="muted">Start a track to see lyrics.</p>
      {:else if lyricsLoading}
        <p class="muted">Loading lyrics…</p>
      {:else if lyricsError}
        <p class="muted">{lyricsError}</p>
      {:else if lyrics?.status === "instrumental"}
        <p class="muted">This track is marked as instrumental.</p>
      {:else if lyrics?.status === "unavailable" || !lyrics}
        <p class="muted">Lyrics are not available for this track.</p>
      {:else if lyrics.synced.length}
        {#each lyrics.synced as line, i (`${line.startMs}-${i}`)}
          <p class="line" class:active={i === activeLine}>{line.text || "♪"}</p>
        {/each}
      {:else if lyrics.plain}
        {#each lyrics.plain.split("\n") as line, i (i)}
          <p class="plain">{line || " "}</p>
        {/each}
      {/if}
    </div>
  </section>

  <aside>
    <header>
      <h3>Queue</h3>
      <button class="refresh muted" onclick={refresh} title="Refresh">⟳</button>
    </header>

    {#if loading && !queue}
      <p class="muted">Loading…</p>
    {:else if queue}
      {#if queue.currentlyPlaying}
        <p class="label muted">Now playing</p>
        <div class="q current">
          <span class="truncate">{queue.currentlyPlaying.name}</span>
          <span class="muted small truncate"
            >{queue.currentlyPlaying.artists.join(", ")}</span
          >
        </div>
      {/if}

      <p class="label muted">Next up</p>
      {#each queue.queue as t, i (t.uri + i)}
        <!-- Jumping into the queue replays it as an explicit track list from
             the chosen entry onward; there is no "skip to queue index" API. -->
        <button
          class="q"
          onclick={() =>
            store.run(() =>
              api.loadTracks(
                queue!.queue.map((x) => x.uri),
                t.uri,
              ),
            )}
        >
          <span class="truncate">{t.name}</span>
          <span class="muted small truncate"
            >{t.artists.join(", ")} · {formatMs(t.durationMs)}</span
          >
        </button>
      {:else}
        <p class="muted small">Queue is empty.</p>
      {/each}
    {/if}
  </aside>
</div>

<style>
  .np {
    position: relative;
    display: grid;
    grid-template-columns: minmax(230px, 0.85fr) minmax(250px, 1fr) minmax(240px, 0.8fr);
    gap: 26px;
    height: 100%;
    padding: 32px;
    overflow: hidden;
  }
  .close {
    position: absolute;
    top: 12px;
    left: 16px;
    color: var(--fg-dim);
    font-size: 18px;
    padding: 4px 10px;
  }
  .art {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 0;
  }
  .art img,
  .ph {
    width: min(340px, 46vh);
    aspect-ratio: 1;
    border-radius: var(--r-md);
    object-fit: cover;
    background: rgba(255, 241, 224, 0.06);
    box-shadow: 0 24px 60px rgba(18, 11, 4, 0.5);
    margin-bottom: 16px;
  }
  .art h2 {
    margin: 0;
    font-size: 22px;
    max-width: 100%;
  }
  .art p {
    margin: 0;
    max-width: 100%;
  }
  .small {
    font-size: 12px;
  }
  aside {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
  }
  .lyrics {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .lyrics header { flex: none; }
  .provider { color: var(--fg-dim); font-size: 10px; letter-spacing: .08em; }
  .lyrics-scroll { min-height: 0; overflow-y: auto; padding: 28vh 8px; scroll-behavior: smooth; mask-image: linear-gradient(transparent, #000 12%, #000 88%, transparent); }
  .line { margin: 0 0 17px; color: var(--fg-dim); font-size: clamp(17px, 2.2vw, 25px); font-weight: 600; line-height: 1.3; transition: color .2s, transform .2s; transform-origin: left center; }
  .line.active { color: var(--fg); transform: scale(1.025); }
  .plain { margin: 0 0 10px; line-height: 1.55; }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 16px;
  }
  .label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    margin: 14px 0 6px;
  }
  .q {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 7px 8px;
    text-align: left;
    min-width: 0;
  }
  .q:hover {
    background: var(--glass-hover);
    border-radius: var(--r-sm);
  }
  .q.current {
    color: var(--accent);
  }
  @media (max-width: 900px) {
    .np { grid-template-columns: minmax(220px, .8fr) minmax(260px, 1fr); overflow-y: auto; }
    aside { grid-column: 1 / -1; max-height: 260px; }
  }
  @media (prefers-reduced-motion: reduce) {
    .lyrics-scroll { scroll-behavior: auto; }
    .line { transition: none; }
  }
</style>
