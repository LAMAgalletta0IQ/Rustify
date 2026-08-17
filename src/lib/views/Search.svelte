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
  <div class="searchwrap" class:empty={!results}>
    <span class="find big">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
        <circle cx="11" cy="11" r="7" /><path d="M20 20l-3.5-3.5" />
      </svg>
      <input
        type="search"
        placeholder="What do you want to listen to?"
        bind:value={query}
        oninput={onInput}
      />
    </span>
  </div>

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
          <button class="card artist" onclick={() => (openArtist = a)}>
            {#if a.imageUrl}<img src={a.imageUrl} alt="" loading="lazy" />{/if}
            <span class="truncate title">{a.name}</span>
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="hint muted">Search across tracks, albums, artists and playlists.</p>
  {/if}
</div>
{/if}

<style>
  .search {
    padding-bottom: 8px;
  }
  /* Centred and dropped down the page until there is something to show, then
     it rises to the top so results get the room. */
  .searchwrap {
    display: grid;
    place-items: center;
    padding: 18px 0 22px;
    transition: padding 0.25s;
  }
  .searchwrap.empty {
    padding-top: 70px;
  }
  .hint {
    text-align: center;
  }
  h3 {
    margin: 24px 0 10px;
    font-size: 15px;
  }
</style>
