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
  let loadingMore = $state(false);
  let error = $state<string | null>(null);
  let offset = $state(0);
  let timer: number | null = null;
  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    brokenImages = new Set(brokenImages).add(url);
  }

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
    error = null;
    offset = 0;
    try {
      results = await api.searchSpotify(q, 10, 0);
    } catch (e) {
      error = store.handleError(e, false).message;
    } finally {
      loading = false;
    }
  }
  async function loadMore() {
    if (!results?.hasMore || loadingMore) return;
    loadingMore = true;
    try {
      const nextOffset = offset + 10;
      const next = await api.searchSpotify(query.trim(), 10, nextOffset);
      const merge = <T extends { uri: string }>(a:T[], b:T[]) => { const seen=new Set(a.map(x=>x.uri)); return [...a,...b.filter(x=>!seen.has(x.uri))]; };
      results = { tracks:merge(results.tracks,next.tracks), albums:merge(results.albums,next.albums), artists:merge(results.artists,next.artists), playlists:merge(results.playlists,next.playlists), hasMore:next.hasMore };
      offset = nextOffset;
    } catch(e) { error=store.handleError(e,false).message; } finally { loadingMore=false; }
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
    {#if error}<div class="search-error" role="alert">{error}</div>{/if}
    {#if results.artists[0] || results.tracks[0]}
      <section class="top-result"><span class="eyebrow">Top result</span>{#if results.artists[0]}<button onclick={() => (openArtist=results!.artists[0])}>{#if results.artists[0].imageUrl && !brokenImages.has(results.artists[0].imageUrl)}<img src={results.artists[0].imageUrl} alt="" onerror={() => onArtworkError(results?.artists[0]?.imageUrl)} />{/if}<strong>{results.artists[0].name}</strong><span>Artist</span></button>{:else}<button onclick={() => store.run(()=>api.loadTracks(results!.tracks.map(t=>t.uri),results!.tracks[0].uri))}><strong>{results.tracks[0].name}</strong><span>{results.tracks[0].artists.join(", ")}</span></button>{/if}</section>
    {/if}
    {#if results.tracks.length}
      <h3>Tracks</h3>
      <TrackList tracks={results.tracks} />
    {/if}

    {#if results.albums.length}
      <h3>Albums</h3>
      <div class="grid">
        {#each results.albums as a (a.id)}
          <button class="card" onclick={() => (openAlbum = a)}>
            {#if a.imageUrl && !brokenImages.has(a.imageUrl)}<img src={a.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(a.imageUrl)} />{/if}
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
            {#if p.imageUrl && !brokenImages.has(p.imageUrl)}<img src={p.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(p.imageUrl)} />{/if}
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
            {#if a.imageUrl && !brokenImages.has(a.imageUrl)}<img src={a.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(a.imageUrl)} />{/if}
            <span class="truncate title">{a.name}</span>
          </button>
        {/each}
      </div>
    {/if}
    {#if results.hasMore}<button class="load-more" disabled={loadingMore} onclick={loadMore}>{loadingMore?"Loading…":"Show more results"}</button>{/if}
    {#if !results.tracks.length && !results.albums.length && !results.artists.length && !results.playlists.length}<div class="search-error">No results for “{query}”. Check the spelling or try fewer words.</div>{/if}
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
  .top-result{margin:8px 0 22px}.eyebrow{color:var(--accent);font-size:10px;text-transform:uppercase;letter-spacing:.08em}.top-result button{display:flex;align-items:center;gap:12px;margin-top:7px;padding:14px 18px;background:var(--glass);border:1px solid var(--hairline);border-radius:var(--r-md);text-align:left}.top-result img{width:58px;height:58px;border-radius:50%;object-fit:cover}.top-result span{color:var(--fg-dim)}.search-error{padding:13px 15px;background:var(--glass);border:1px solid var(--hairline);border-radius:var(--r-md);color:var(--fg-dim)}.load-more{display:block;margin:22px auto 4px;padding:9px 14px;border:1px solid var(--hairline);background:var(--glass);border-radius:999px}
</style>
