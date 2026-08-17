<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import AlbumView from "./AlbumView.svelte";
  import ArtistView from "./ArtistView.svelte";
  import type { AlbumSummary, ArtistSummary, SearchResults } from "../types";

  /** Drill-down stacked over the results; null shows the results. */
  let openAlbum = $state<AlbumSummary | null>(null);
  let openArtist = $state<ArtistSummary | null>(null);

  let query = $state("");
  let results = $state<SearchResults | null>(null);
  let loading = $state(false);
  let timer: number | null = null;

  /** Debounced so typing doesn't fire a request per keystroke. */
  function onInput() {
    if (timer !== null) clearTimeout(timer);
    timer = window.setTimeout(run, 300);
  }

  async function run() {
    const q = query.trim();
    if (!q) {
      results = null;
      return;
    }
    loading = true;
    try {
      results = await api.searchSpotify(q);
    } catch (e) {
      store.error = api.asAppError(e).message;
    } finally {
      loading = false;
    }
  }
</script>

{#if openAlbum}
  <AlbumView album={openAlbum} onBack={() => (openAlbum = null)} />
{:else if openArtist}
  <ArtistView
    artist={openArtist}
    onBack={() => (openArtist = null)}
    onOpenAlbum={(a) => {
      openArtist = null;
      openAlbum = a;
    }}
  />
{:else}
<div class="search">
  <input
    type="search"
    placeholder="Search tracks, albums, artists, playlists…"
    bind:value={query}
    oninput={onInput}
  />

  {#if loading}
    <p class="muted">Searching…</p>
  {:else if results}
    {#if results.tracks.length}
      <h3>Tracks</h3>
      <TrackList tracks={results.tracks} />
    {/if}

    {#if results.albums.length}
      <h3>Albums</h3>
      <div class="grid">
        {#each results.albums as a (a.id)}
          <button class="card" onclick={() => (openAlbum = a)}>
            {#if a.imageUrl}<img src={a.imageUrl} alt="" loading="lazy" />{/if}
            <span class="truncate title">{a.name}</span>
            <span class="truncate muted sub">{a.artists.join(", ")}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#if results.playlists.length}
      <h3>Playlists</h3>
      <div class="grid">
        {#each results.playlists as p (p.id)}
          <button class="card" onclick={() => store.run(() => api.loadContext(p.uri))}>
            {#if p.imageUrl}<img src={p.imageUrl} alt="" loading="lazy" />{/if}
            <span class="truncate title">{p.name}</span>
            <span class="truncate muted sub">{p.owner}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#if results.artists.length}
      <h3>Artists</h3>
      <div class="grid">
        {#each results.artists as a (a.id)}
          <button class="card" onclick={() => (openArtist = a)}>
            {#if a.imageUrl}<img class="round" src={a.imageUrl} alt="" loading="lazy" />{/if}
            <span class="truncate title">{a.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="muted">Type to search.</p>
  {/if}
</div>
{/if}

<style>
  .search {
    padding: 20px 24px 8px;
  }
  input {
    width: 100%;
    max-width: 520px;
    margin-bottom: 20px;
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
  .card img {
    width: 100%;
    aspect-ratio: 1;
    border-radius: 6px;
    object-fit: cover;
    margin-bottom: 6px;
  }
  .card img.round {
    border-radius: 50%;
  }
  .title {
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
  }
</style>
