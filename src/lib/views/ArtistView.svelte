<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import type { AlbumSummary, ArtistSummary, TrackSummary } from "../types";

  let {
    artist,
    onBack,
    onOpenAlbum,
  }: {
    artist: ArtistSummary;
    onBack: () => void;
    onOpenAlbum: (a: AlbumSummary) => void;
  } = $props();

  let top = $state<TrackSummary[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let loading = $state(true);

  $effect(() => {
    const id = artist.id;
    let cancelled = false;
    loading = true;
    (async () => {
      try {
        const [t, a] = await Promise.all([
          api.getArtistTopTracks(id),
          api.getArtistAlbums(id),
        ]);
        if (cancelled) return;
        top = t;
        albums = a;
      } catch (e) {
        if (!cancelled) store.error = api.asAppError(e).message;
      } finally {
        if (!cancelled) loading = false;
      }
    })();
    return () => {
      cancelled = true;
    };
  });
</script>

<div class="artist">
  <div class="head">
    <button class="back" onclick={onBack}>← Back</button>
    {#if artist.imageUrl}
      <img class="avatar" src={artist.imageUrl} alt="" />
    {/if}
    <h2 class="truncate">{artist.name}</h2>
    <button
      class="btn-primary"
      onclick={() => store.run(() => api.loadContext(artist.uri))}>Play</button
    >
  </div>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    {#if top.length}
      <h3>Popular</h3>
      <!-- No container context: top tracks are a synthesised list. -->
      <TrackList tracks={top} />
    {/if}

    {#if albums.length}
      <h3>Albums &amp; singles</h3>
      <div class="grid">
        {#each albums as a (a.id)}
          <button class="card" onclick={() => onOpenAlbum(a)}>
            {#if a.imageUrl}
              <img src={a.imageUrl} alt="" loading="lazy" />
            {:else}
              <span class="ph"></span>
            {/if}
            <span class="truncate title">{a.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .artist {
    padding: 20px 24px 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 16px;
  }
  .head h2 {
    margin: 0;
    flex: 1;
    font-size: 22px;
  }
  .avatar {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    object-fit: cover;
  }
  .back {
    color: var(--fg-dim);
  }
  h3 {
    margin: 20px 0 8px;
    font-size: 15px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 14px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px;
    background: var(--bg-elev);
    text-align: left;
  }
  .card:hover {
    background: var(--bg-elev-2);
  }
  .card img,
  .ph {
    width: 100%;
    aspect-ratio: 1;
    border-radius: 6px;
    object-fit: cover;
    background: var(--bg-elev-2);
    margin-bottom: 6px;
  }
  .title {
    font-weight: 600;
  }
</style>
