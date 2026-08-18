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

  let loadedDetails = $state<ArtistSummary | null>(null);
  const details = $derived(loadedDetails ?? artist);
  let tracks = $state<TrackSummary[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let loadingTracks = $state(true);
  let loadingAlbums = $state(true);
  let loadingMore = $state(false);
  let tracksError = $state<string | null>(null);
  let albumsError = $state<string | null>(null);
  let hasMore = $state(false);
  let reloadKey = $state(0);

  $effect(() => {
    const id = artist.id;
    reloadKey;
    let cancelled = false;
    loadingTracks = loadingAlbums = true;
    tracksError = albumsError = null;

    void Promise.allSettled([
      api.getArtist(id).then((value) => {
        if (!cancelled) loadedDetails = value;
      }),
      api
        .getArtistTopTracks(id)
        .then((value) => {
          if (!cancelled) tracks = value;
        })
        .catch((e) => {
          if (!cancelled) tracksError = store.handleError(e, false).message;
        })
        .finally(() => {
          if (!cancelled) loadingTracks = false;
        }),
      api
        .getArtistAlbums(id, 10, 0)
        .then((page) => {
          if (!cancelled) {
            albums = page.items;
            hasMore = page.hasMore;
          }
        })
        .catch((e) => {
          if (!cancelled) albumsError = store.handleError(e, false).message;
        })
        .finally(() => {
          if (!cancelled) loadingAlbums = false;
        }),
    ]);
    return () => {
      cancelled = true;
    };
  });

  async function loadMore() {
    if (loadingMore || !hasMore) return;
    loadingMore = true;
    albumsError = null;
    try {
      const page = await api.getArtistAlbums(artist.id, 10, albums.length);
      const seen = new Set(albums.map((album) => album.uri));
      albums = [...albums, ...page.items.filter((album) => !seen.has(album.uri))];
      hasMore = page.hasMore;
    } catch (e) {
      albumsError = store.handleError(e, false).message;
    } finally {
      loadingMore = false;
    }
  }
</script>

<div class="artist">
  <div class="head">
    <button class="back" onclick={onBack}>← Back</button>
    {#if details.imageUrl}<img class="avatar" src={details.imageUrl} alt="" />{:else}<span class="avatar ph"></span>{/if}
    <span class="identity"><span class="muted kind">Artist</span><h2 class="truncate">{details.name}</h2></span>
    <button class="btn-primary" onclick={() => store.run(() => api.loadContext(details.uri))}>Play</button>
  </div>

  <section aria-labelledby="tracks-heading">
    <h3 id="tracks-heading">Tracks from recent releases</h3>
    <p class="note">Spotify no longer exposes artist popularity rankings to this integration.</p>
    {#if loadingTracks}
      <p class="muted">Loading tracks…</p>
    {:else if tracksError}
      <div class="state"><span>{tracksError}</span><button onclick={() => reloadKey++}>Retry</button></div>
    {:else if tracks.length}
      <TrackList {tracks} />
    {:else}
      <p class="muted">No playable tracks were found in this artist’s recent releases.</p>
    {/if}
  </section>

  <section aria-labelledby="releases-heading">
    <h3 id="releases-heading">Albums &amp; singles</h3>
    {#if loadingAlbums}
      <p class="muted">Loading releases…</p>
    {:else if albums.length}
      <div class="grid">
        {#each albums as album (album.uri)}
          <button class="card" onclick={() => onOpenAlbum(album)}>
            {#if album.imageUrl}<img src={album.imageUrl} alt="" loading="lazy" />{:else}<span class="ph cover"></span>{/if}
            <span class="truncate title">{album.name}</span>
            <span class="truncate sub">{album.artists.join(", ")}</span>
          </button>
        {/each}
      </div>
      {#if hasMore}<button class="more" disabled={loadingMore} onclick={loadMore}>{loadingMore ? "Loading…" : "Load more"}</button>{/if}
    {:else if !albumsError}
      <p class="muted">No albums or singles are available.</p>
    {/if}
    {#if albumsError}<div class="state"><span>{albumsError}</span><button onclick={() => reloadKey++}>Retry</button></div>{/if}
  </section>
</div>

<style>
  .artist { padding: 20px 24px 8px; }
  .head { display: flex; align-items: center; gap: 16px; margin-bottom: 22px; }
  .identity { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .head h2 { margin: 0; font-size: 24px; }
  .kind { font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }
  .avatar { width: 72px; height: 72px; border-radius: 50%; object-fit: cover; flex: none; }
  .ph { background: rgba(255,241,224,.06); }
  .back { color: var(--fg-dim); }
  section { margin-top: 24px; }
  h3 { margin: 0 0 5px; font-size: 16px; }
  .note { margin: 0 0 10px; color: var(--fg-dim); font-size: 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px,1fr)); gap: 14px; }
  .cover { display: block; width: 100%; aspect-ratio: 1; border-radius: 11px; margin-bottom: 11px; }
  .state { display: flex; justify-content: space-between; gap: 16px; padding: 12px 14px; border: 1px solid var(--hairline); background: var(--glass); border-radius: var(--r-md); color: var(--fg-dim); }
  .state button, .more { color: var(--fg); text-decoration: underline; text-underline-offset: 2px; }
  .more { display: block; margin: 18px auto 0; padding: 8px 14px; }
</style>
