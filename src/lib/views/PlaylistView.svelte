<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import { trackCountLabel } from "../types";
  import type { PlaylistSummary, TrackSummary } from "../types";

  let { playlist, onBack }: { playlist: PlaylistSummary; onBack: () => void } =
    $props();

  /** The Web API caps playlist tracks at 100 per request, not the usual 50. */
  const PAGE = 100;

  let tracks = $state<TrackSummary[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  /** A short page means the server ran out; nothing more to ask for. */
  let exhausted = $state(false);

  $effect(() => {
    const id = playlist.id;
    let cancelled = false;
    loading = true;
    (async () => {
      try {
        const t = await api.getPlaylistTracks(id, PAGE, 0);
        if (!cancelled) {
          tracks = t;
          exhausted = t.length < PAGE;
        }
      } catch (e) {
        if (!cancelled) store.handleError(e);
      } finally {
        if (!cancelled) loading = false;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  async function loadMore() {
    if (loadingMore || exhausted) return;
    loadingMore = true;
    try {
      const next = await api.getPlaylistTracks(playlist.id, PAGE, tracks.length);
      tracks = [...tracks, ...next];
      exhausted = next.length < PAGE;
    } catch (e) {
      store.handleError(e);
    } finally {
      loadingMore = false;
    }
  }
</script>

<div class="view">
  <div class="head glass">
    <button class="back" onclick={onBack} title="Back">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M15 5l-7 7 7 7" /></svg>
    </button>
    {#if playlist.imageUrl}
      <img src={playlist.imageUrl} alt="" />
    {:else}
      <span class="ph"></span>
    {/if}
    <span class="meta">
      <span class="truncate name">{playlist.name}</span>
      <span class="truncate muted">
        {playlist.owner} · {trackCountLabel(playlist.trackCount)}
      </span>
    </span>
    <button
      class="btn-primary"
      onclick={() => store.run(() => api.loadContext(playlist.uri))}
      disabled={tracks.length === 0}>Play</button
    >
  </div>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    <TrackList {tracks} contextUri={playlist.uri} />
    {#if !exhausted}
      <button class="more" onclick={loadMore} disabled={loadingMore}>
        {loadingMore ? "Loading…" : "Load more"}
      </button>
    {/if}
  {/if}
</div>

<style>
  .view {
    padding-top: 18px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 18px;
    border-radius: var(--r-md);
    margin-bottom: 18px;
  }
  .head img,
  .ph {
    width: 84px;
    height: 84px;
    border-radius: 11px;
    object-fit: cover;
    flex: none;
    background: rgba(255, 241, 224, 0.06);
  }
  .meta {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    gap: 2px;
  }
  .name {
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.3px;
  }
  .back {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    color: var(--fg-dim);
    flex: none;
  }
  .back:hover {
    background: var(--glass-hover);
    color: var(--fg);
  }
  .more {
    display: block;
    margin: 20px auto 8px;
    padding: 8px 22px;
    border-radius: 999px;
    color: var(--fg-dim);
    background: var(--glass);
    border: 1px solid var(--hairline);
  }
  .more:hover:not(:disabled) {
    color: var(--fg);
  }
</style>
