<script lang="ts">
  import { untrack } from "svelte";
  import * as api from "../api";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import AlbumView from "./AlbumView.svelte";
  import ArtistView from "./ArtistView.svelte";
  import PlaylistView from "./PlaylistView.svelte";
  import { trackCountLabel } from "../types";
  import type {
    AlbumSummary,
    ArtistSummary,
    PlaylistSummary,
    TrackSummary,
  } from "../types";

  type Section = "playlists" | "albums" | "artists" | "liked";

  const PAGE = 50;

  let section = $state<Section>("playlists");
  let filter = $state("");
  let alpha = $state(false);

  let playlists = $state<PlaylistSummary[]>([]);
  let albums = $state<AlbumSummary[]>([]);
  let artists = $state<ArtistSummary[]>([]);
  let liked = $state<TrackSummary[]>([]);

  let loading = $state(false);
  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    brokenImages = new Set(brokenImages).add(url);
  }
  let loadingMore = $state(false);

  /**
   * A short page means the server ran out. Tracked per section since each
   * paginates independently. Artists are excluded — they page by cursor, so
   * `artistCursor` carries that state instead.
   */
  let exhausted = $state<Record<Section, boolean>>({
    playlists: false,
    albums: false,
    artists: false,
    liked: false,
  });
  /** Cursor for the next page of followed artists; null once exhausted. */
  let artistCursor = $state<string | null>(null);

  /** Drill-downs stacked over the grid; all null shows the grid. */
  let openPlaylist = $state<PlaylistSummary | null>(null);
  let openAlbum = $state<AlbumSummary | null>(null);
  let openArtist = $state<ArtistSummary | null>(null);

  const likedUri = $derived(
    store.auth.userId ? api.likedSongsUri(store.auth.userId) : null,
  );

  /** Client-side narrowing over what is already loaded. */
  function sift<T extends { name: string }>(items: T[]): T[] {
    const q = filter.trim().toLowerCase();
    const out = q
      ? items.filter((i) => i.name.toLowerCase().includes(q))
      : [...items];
    if (alpha) out.sort((a, b) => a.name.localeCompare(b.name));
    return out;
  }

  const shownPlaylists = $derived(sift(playlists));
  const shownAlbums = $derived(sift(albums));
  const shownArtists = $derived(sift(artists));
  const shownLiked = $derived(sift(liked));

  /** First page for the active section, fetched once and then cached. */
  async function load() {
    loading = true;
    try {
      if (section === "playlists" && playlists.length === 0) {
        playlists = await api.getPlaylists(PAGE, 0);
        exhausted.playlists = playlists.length < PAGE;
      }
      if (section === "albums" && albums.length === 0) {
        albums = await api.getSavedAlbums(PAGE, 0);
        exhausted.albums = albums.length < PAGE;
      }
      if (section === "artists" && artists.length === 0) {
        const page = await api.getFollowedArtists(PAGE);
        artists = page.items;
        artistCursor = page.next;
      }
      if (section === "liked" && liked.length === 0) {
        liked = await api.getSavedTracks(PAGE, 0);
        exhausted.liked = liked.length < PAGE;
      }
    } catch (e) {
      store.handleError(e);
    } finally {
      loading = false;
    }
  }

  async function loadMore() {
    if (loadingMore) return;
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
      } else if (section === "artists") {
        // Cursor, not offset: hand back what the last page returned.
        if (!artistCursor) return;
        const page = await api.getFollowedArtists(PAGE, artistCursor);
        artists = [...artists, ...page.items];
        artistCursor = page.next;
      } else {
        const next = await api.getSavedTracks(PAGE, liked.length);
        liked = [...liked, ...next];
        exhausted.liked = next.length < PAGE;
      }
    } catch (e) {
      store.handleError(e);
    } finally {
      loadingMore = false;
    }
  }

  const canLoadMore = $derived(
    section === "artists" ? artistCursor !== null : !exhausted[section],
  );

  function pick(next: Section) {
    section = next;
    filter = "";
    openPlaylist = null;
    openAlbum = null;
    openArtist = null;
  }

  // Load on mount and whenever the section changes. `load` reads the cached
  // arrays, so it must run untracked or appending a page would re-trigger it.
  $effect(() => {
    section;
    untrack(load);
  });
</script>

{#if openPlaylist}
  <PlaylistView
    playlist={openPlaylist}
    onBack={() => (openPlaylist = null)}
    onEdited={(updated) => {
      // Keep the card behind the view in sync, so backing out doesn't show the
      // old name until the next full library refetch.
      playlists = playlists.map((p) => (p.id === updated.id ? updated : p));
      openPlaylist = updated;
    }}
  />
{:else if openAlbum}
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
  <div class="library">
    <div class="head">
      <h1>Library</h1>
      <div class="tools">
        <span class="find">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <circle cx="11" cy="11" r="7" /><path d="M20 20l-3.5-3.5" />
          </svg>
          <input placeholder="Filter…" bind:value={filter} aria-label="Filter library" />
        </span>
        <button
          class="chip"
          onclick={() => (alpha = !alpha)}
          title="Toggle sort order"
        >
          {alpha ? "A–Z" : "Recents"}
        </button>
      </div>
    </div>

    <div class="chips">
      <button class="chip" class:on={section === "playlists"} onclick={() => pick("playlists")}>
        Playlists
      </button>
      <button class="chip" class:on={section === "albums"} onclick={() => pick("albums")}>
        Albums
      </button>
      <button class="chip" class:on={section === "artists"} onclick={() => pick("artists")}>
        Artists
      </button>
      <button class="chip" class:on={section === "liked"} onclick={() => pick("liked")}>
        Liked Songs
      </button>
    </div>

    <div class="body">
      {#if loading}
        <p class="muted">Loading…</p>
      {:else if section === "liked"}
        <TrackList tracks={shownLiked} contextUri={likedUri} />
      {:else if section === "playlists"}
        <div class="grid">
          {#each shownPlaylists as p (p.id)}
            <button class="card" onclick={() => (openPlaylist = p)}>
              {#if p.imageUrl && !brokenImages.has(p.imageUrl)}
                <img src={p.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(p.imageUrl)} />
              {:else}
                <span class="art"></span>
              {/if}
              <span class="truncate title">{p.name}</span>
              <span class="truncate sub">{trackCountLabel(p.trackCount)}</span>
            </button>
          {:else}
            <p class="muted">{filter ? "Nothing matches." : "No playlists."}</p>
          {/each}
        </div>
      {:else if section === "albums"}
        <div class="grid">
          {#each shownAlbums as a (a.id)}
            <button class="card" onclick={() => (openAlbum = a)}>
              {#if a.imageUrl && !brokenImages.has(a.imageUrl)}
                <img src={a.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(a.imageUrl)} />
              {:else}
                <span class="art"></span>
              {/if}
              <span class="truncate title">{a.name}</span>
              <span class="truncate sub">{a.artists.join(", ")}</span>
            </button>
          {:else}
            <p class="muted">{filter ? "Nothing matches." : "No saved albums."}</p>
          {/each}
        </div>
      {:else}
        <div class="grid">
          {#each shownArtists as a (a.id)}
            <button class="card artist" onclick={() => (openArtist = a)}>
              {#if a.imageUrl && !brokenImages.has(a.imageUrl)}
                <img src={a.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(a.imageUrl)} />
              {:else}
                <span class="art"></span>
              {/if}
              <!-- No "Artist" sublabel: the circle already says so, and in this
                   section every tile is one. -->
              <span class="truncate title">{a.name}</span>
            </button>
          {:else}
            <p class="muted">
              {filter ? "Nothing matches." : "You don't follow any artists yet."}
            </p>
          {/each}
        </div>
      {/if}

      {#if !loading && canLoadMore}
        <button class="more" onclick={loadMore} disabled={loadingMore}>
          {loadingMore ? "Loading…" : "Load more"}
        </button>
      {/if}
    </div>
  </div>
{/if}

<style>
  .library {
    padding-bottom: 8px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    flex-wrap: wrap;
  }
  h1 {
    font-size: 30px;
    font-weight: 600;
    letter-spacing: -0.5px;
    margin: 18px 0;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .body {
    margin-top: 20px;
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
