<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "../api";
  import { store } from "../store.svelte";
  import { formatMs, type QueueView } from "../types";

  let { onClose }: { onClose: () => void } = $props();

  let queue = $state<QueueView | null>(null);
  let loading = $state(false);

  const pb = $derived(store.playback);

  async function refresh() {
    loading = true;
    try {
      queue = await api.getQueue();
    } catch (e) {
      store.error = api.asAppError(e).message;
    } finally {
      loading = false;
    }
  }

  // Refresh the queue whenever the track changes.
  $effect(() => {
    pb.track?.uri;
    untrack(refresh);
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

    <!-- Lyrics deliberately absent: Spotify's public Web API exposes no
         lyrics endpoint. Space is reserved here for a future provider. -->
  </div>

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
    grid-template-columns: 1fr 300px;
    gap: 32px;
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
    border-radius: 10px;
    object-fit: cover;
    background: var(--bg-elev-2);
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
    background: var(--bg-elev-2);
  }
  .q.current {
    color: var(--accent);
  }
</style>
