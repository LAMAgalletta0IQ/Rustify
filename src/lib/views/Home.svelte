<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "../api";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import type { AlbumSummary, PlaylistSummary, TrackSummary } from "../types";

  type Section = "playlists" | "albums" | "liked";

  const PAGE = 50;
  /** The Web API caps playlist tracks at 100 per request, not 50. */
  const TRACK_PAGE = 100;

  let section = $state<Section>("playlists");
  let playlists = $state<PlaylistSummary[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let liked = $state<TrackSummary[]>([]);
  let loading = $state(false);
  let loadingMore = $state(false);

  /**
   * A short page means the server ran out, so there is nothing more to ask
   * for. Tracked per section since each paginates independently.
   */
  let exhausted = $state<Record<Section, boolean>>({
    playlists: false,
    albums: false,
    liked: false,
  });

  /** Set when a playlist/album is opened; null shows the grid. */
  let detail = $state<{
    title: string;
    /** Container URI, so playback continues past the clicked track. */
    uri: string;
    tracks: TrackSummary[];
    /** Null for albums — album tracks come back in one request. */
    playlistId: string | null;
    exhausted: boolean;
  } | null>(null);

  /** Liked Songs is itself a playable context. */
  const likedUri = $derived(
    store.auth.userId ? api.likedSongsUri(store.auth.userId) : null,
  );

  /** First page for the active section, fetched once and then cached. */
  async function load() {
    loading = true;
    detail = null;
    try {
      if (section === "playlists" && playlists.length === 0) {
        playlists = await api.getPlaylists(PAGE, 0);
        exhausted.playlists = playlists.length < PAGE;
      }
      if (section === "albums" && albums.length === 0) {
        albums = await api.getSavedAlbums(PAGE, 0);
        exhausted.albums = albums.length < PAGE;
      }
      if (section === "liked" && liked.length === 0) {
        liked = await api.getSavedTracks(PAGE, 0);
        exhausted.liked = liked.length < PAGE;
      }
    } catch (e) {
      store.error = api.asAppError(e).message;
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (loadingMore || exhausted[section]) return;
    loadingMore = true;
    try {
      if (section === "playlists") {
        const next = await api.getPlaylists(PAGE, playlists.length);
        playlists = [...playlists, ...next];
        exhausted.playlists = next.length < PAGE;
      } else if (section === "albums") {
        const next = await api.getSavedAlbums(PAGE, albums.length);
        albums = [...albums, ...next];
        exhausted.albums = next.length < PAGE;
      } else {
        const next = await api.getSavedTracks(PAGE, liked.length);
        liked = [...liked, ...next];
        exhausted.liked = next.length < PAGE;
      }
    } catch (e) {
      store.error = api.asAppError(e).message;
    } finally {
      loadingMore = false;
    }
  }

  /** Pulls the next page of tracks inside an open playlist. */
  async function loadMoreTracks() {
    if (!detail?.playlistId || detail.exhausted || loadingMore) return;
    loadingMore = true;
    try {
      const next = await api.getPlaylistTracks(
        detail.playlistId,
        TRACK_PAGE,
        detail.tracks.length,
      );
      detail.tracks = [...detail.tracks, ...next];
      detail.exhausted = next.length < TRACK_PAGE;
    } catch (e) {
      store.error = api.asAppError(e).message;
    } finally {
      loadingMore = false;
    }
  }

  async function openPlaylist(p: PlaylistSummary) {
    loading = true;
    try {
      const tracks = await api.getPlaylistTracks(p.id, TRACK_PAGE, 0);
      detail = {
        title: p.name,
        uri: p.uri,
        tracks,
        playlistId: p.id,
        exhausted: tracks.length < TRACK_PAGE,
      };
    } catch (e) {
      store.error = api.asAppError(e).message;
    } finally {
      loading = false;
    }
  }

  async function openAlbum(a: AlbumSummary) {
    loading = true;
    try {
      detail = {
        title: a.name,
        uri: a.uri,
        tracks: await api.getAlbumTracks(a.id),
        playlistId: null,
        exhausted: true,
      };
    } catch (e) {
      store.error = api.asAppError(e).message;
    } finally {
      loading = false;
    }
  }

  // Load on mount and whenever the section changes. `load` reads the cached
  // arrays, so it must run untracked or appending a page would re-trigger it.
  $effect(() => {
    section;
    untrack(load);
  });
</script>

<div class="home">
  <nav>
    <button class:on={section === "playlists"} onclick={() => (section = "playlists")}>
      Playlists
    </button>
    <button class:on={section === "albums"} onclick={() => (section = "albums")}>
      Albums
    </button>
    <button class:on={section === "liked"} onclick={() => (section = "liked")}>
      Liked Songs
    </button>
  </nav>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if detail}
    <div class="dhead">
      <button class="back" onclick={() => (detail = null)}>← Back</button>
      <h2>{detail.title}</h2>
      <button
        class="btn-primary"
        onclick={() => store.run(() => api.loadContext(detail!.uri))}
        disabled={detail.tracks.length === 0}>Play</button
      >
    </div>
    <TrackList tracks={detail.tracks} contextUri={detail.uri} />
    {#if !detail.exhausted}
      <button class="more" onclick={loadMoreTracks} disabled={loadingMore}>
        {loadingMore ? "Loading…" : "Load more"}
      </button>
    {/if}
  {:else if section === "liked"}
    <TrackList tracks={liked} contextUri={likedUri} />
    {#if !exhausted.liked}
      <button class="more" onclick={loadMore} disabled={loadingMore}>
        {loadingMore ? "Loading…" : "Load more"}
      </button>
    {/if}
  {:else if section === "playlists"}
    <div class="grid">
      {#each playlists as p (p.id)}
        <button class="card" onclick={() => openPlaylist(p)}>
          {#if p.imageUrl}
            <img src={p.imageUrl} alt="" loading="lazy" />
          {:else}
            <span class="ph"></span>
          {/if}
          <span class="truncate title">{p.name}</span>
          <span class="truncate muted sub">{p.trackCount} tracks</span>
        </button>
      {:else}
        <p class="muted">No playlists.</p>
      {/each}
    </div>
    {#if !exhausted.playlists}
      <button class="more" onclick={loadMore} disabled={loadingMore}>
        {loadingMore ? "Loading…" : "Load more"}
      </button>
    {/if}
  {:else}
    <div class="grid">
      {#each albums as a (a.id)}
        <button class="card" onclick={() => openAlbum(a)}>
          {#if a.imageUrl}
            <img src={a.imageUrl} alt="" loading="lazy" />
          {:else}
            <span class="ph"></span>
          {/if}
          <span class="truncate title">{a.name}</span>
          <span class="truncate muted sub">{a.artists.join(", ")}</span>
        </button>
      {:else}
        <p class="muted">No saved albums.</p>
      {/each}
    </div>
    {#if !exhausted.albums}
      <button class="more" onclick={loadMore} disabled={loadingMore}>
        {loadingMore ? "Loading…" : "Load more"}
      </button>
    {/if}
  {/if}
</div>

<style>
  .home {
    padding: 20px 24px 8px;
  }
  .more {
    display: block;
    margin: 20px auto 8px;
    padding: 8px 20px;
    border: 1px solid var(--border);
    color: var(--fg-dim);
  }
  .more:hover:not(:disabled) {
    color: var(--fg);
    border-color: var(--fg-dim);
  }
  nav {
    display: flex;
    gap: 8px;
    margin-bottom: 20px;
  }
  nav button {
    padding: 6px 14px;
    border-radius: 999px;
    background: var(--bg-elev-2);
    color: var(--fg-dim);
  }
  nav button.on {
    background: var(--fg);
    color: var(--bg);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 16px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px;
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
    background: var(--bg-elev-2);
    object-fit: cover;
    margin-bottom: 6px;
  }
  .title {
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
  }
  .dhead {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 12px;
  }
  .dhead h2 {
    margin: 0;
    flex: 1;
    font-size: 20px;
  }
  .back {
    color: var(--fg-dim);
  }
</style>
