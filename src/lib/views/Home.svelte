<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type {
    AlbumSummary,
    ArtistSummary,
    PlaylistSummary,
    RecentActivityItem,
    TrackSummary,
  } from "../types";
  import AlbumView from "./AlbumView.svelte";
  import ArtistView from "./ArtistView.svelte";
  import PlaylistView from "./PlaylistView.svelte";

  let { onBrowseLibrary }: { onBrowseLibrary: () => void } = $props();

  const QUICK_LIMIT = 6;
  const RECENT_LIMIT = 12;
  const MIX_LIMIT = 12;

  let playlists = $state<PlaylistSummary[]>([]);
  let recent = $state<RecentActivityItem[]>([]);
  let topTracks = $state<TrackSummary[]>([]);
  let discovery = $state<AlbumSummary[]>([]);
  let loadingRecent = $state(true);
  let loadingMix = $state(true);
  let loadingDiscovery = $state(true);
  let recentError = $state<string | null>(null);
  let mixError = $state<string | null>(null);
  let discoveryError = $state<string | null>(null);
  let reloadKey = $state(0);

  let openPlaylist = $state<PlaylistSummary | null>(null);
  let openAlbum = $state<AlbumSummary | null>(null);
  let openArtist = $state<ArtistSummary | null>(null);

  const greeting = $derived.by(() => {
    const h = new Date().getHours();
    if (h < 5) return "Still up";
    if (h < 12) return "Good morning";
    if (h < 18) return "Good afternoon";
    return "Good evening";
  });

  const deviceLabel = $derived(
    store.playback.isActiveDevice ? "this device" : "another device",
  );

  const quickAccess = $derived.by(() => {
    const items: RecentActivityItem[] = [];
    const seen = new Set<string>();
    for (const item of recent) {
      if (seen.has(item.uri)) continue;
      seen.add(item.uri);
      const playlist =
        item.kind === "playlist"
          ? playlists.find((candidate) => candidate.uri === item.uri)
          : undefined;
      items.push(
        playlist
          ? {
              ...item,
              name: playlist.name,
              subtitle: playlist.owner,
              imageUrl: playlist.imageUrl,
            }
          : item,
      );
      if (items.length === QUICK_LIMIT) return items;
    }
    for (const playlist of playlists) {
      if (seen.has(playlist.uri)) continue;
      seen.add(playlist.uri);
      items.push({
        kind: "playlist",
        uri: playlist.uri,
        id: playlist.id,
        name: playlist.name,
        subtitle: playlist.owner,
        imageUrl: playlist.imageUrl,
        lastPlayedAt: "",
        trackUri: null,
      });
      if (items.length === QUICK_LIMIT) break;
    }
    return items;
  });

  $effect(() => {
    reloadKey;
    let cancelled = false;
    loadingRecent = true;
    loadingMix = true;
    loadingDiscovery = true;
    recentError = mixError = discoveryError = null;

    const recentTask = Promise.allSettled([
      api.getPlaylists(50, 0),
      api.getRecentlyPlayed(50),
    ]).then(([playlistResult, recentResult]) => {
      if (cancelled) return;
      if (playlistResult.status === "fulfilled") playlists = playlistResult.value;
      if (recentResult.status === "fulfilled") {
        recent = recentResult.value.slice(0, RECENT_LIMIT);
      } else {
        recentError = store.handleError(recentResult.reason, false).message;
      }
      loadingRecent = false;
    });

    const mixTask = api
      .getTopTracks(MIX_LIMIT)
      .then((tracks) => {
        if (!cancelled) topTracks = tracks;
      })
      .catch((e) => {
        if (!cancelled) mixError = store.handleError(e, false).message;
      })
      .finally(() => {
        if (!cancelled) loadingMix = false;
      });

    const discoveryTask = api
      .getTopArtists(4)
      .then(async (artists) => {
        const pages = await Promise.allSettled(
          artists.map((artist) => api.getArtistAlbums(artist.id, 4, 0)),
        );
        if (cancelled) return;
        const seen = new Set<string>();
        discovery = pages
          .flatMap((page) =>
            page.status === "fulfilled" ? page.value.items : [],
          )
          .filter((album) => seen.add(album.uri))
          .slice(0, 12);
        if (!discovery.length && pages.some((page) => page.status === "rejected")) {
          const failed = pages.find((page) => page.status === "rejected");
          if (failed?.status === "rejected") {
            discoveryError = store.handleError(failed.reason, false).message;
          }
        }
      })
      .catch((e) => {
        if (!cancelled) discoveryError = store.handleError(e, false).message;
      })
      .finally(() => {
        if (!cancelled) loadingDiscovery = false;
      });

    void Promise.all([recentTask, mixTask, discoveryTask]);
    return () => {
      cancelled = true;
    };
  });

  function playlistFor(item: RecentActivityItem): PlaylistSummary | null {
    return playlists.find((playlist) => playlist.uri === item.uri) ?? null;
  }

  function openActivity(item: RecentActivityItem) {
    if (item.kind === "album") {
      openAlbum = {
        uri: item.uri,
        id: item.id,
        name: item.name,
        artists: item.subtitle ? [item.subtitle] : [],
        imageUrl: item.imageUrl,
      };
    } else if (item.kind === "artist") {
      openArtist = {
        uri: item.uri,
        id: item.id,
        name: item.name,
        imageUrl: item.imageUrl,
      };
    } else if (item.kind === "playlist" && playlistFor(item)) {
      openPlaylist = playlistFor(item);
    } else if (item.kind === "track") {
      const tracks = recent.filter((x) => x.kind === "track").map((x) => x.uri);
      store.run(() => api.loadTracks(tracks, item.uri));
    } else {
      store.run(() => api.loadContext(item.uri, item.trackUri ?? undefined));
    }
  }

  function playRecent(item: RecentActivityItem) {
    if (item.kind === "track") {
      const tracks = recent.filter((x) => x.kind === "track").map((x) => x.uri);
      store.run(() => api.loadTracks(tracks, item.uri));
    } else {
      store.run(() => api.loadContext(item.uri, item.trackUri ?? undefined));
    }
  }

  function playMix(track: TrackSummary) {
    store.run(() => api.loadTracks(topTracks.map((x) => x.uri), track.uri));
  }
</script>

{#if openPlaylist}
  <PlaylistView playlist={openPlaylist} onBack={() => (openPlaylist = null)} />
{:else if openAlbum}
  <AlbumView album={openAlbum} onBack={() => (openAlbum = null)} />
{:else if openArtist}
  <ArtistView
    artist={openArtist}
    onBack={() => (openArtist = null)}
    onOpenAlbum={(album) => (openAlbum = album)}
  />
{:else}
  <div class="home">
    <h1 class="greet">{greeting}</h1>
    <p class="greet-sub muted">
      {#if store.playback.track}
        Playing on <strong>{deviceLabel}</strong>
      {:else}
        Nothing playing
      {/if}
    </p>

    {#if quickAccess.length}
      <div class="jump" aria-label="Quick access">
        {#each quickAccess as item (item.uri)}
          <button class="pill" onclick={() => openActivity(item)}>
            {#if item.imageUrl}
              <img src={item.imageUrl} alt="" loading="lazy" />
            {:else}
              <span class="art"></span>
            {/if}
            <span class="truncate label">{item.name}</span>
          </button>
        {/each}
      </div>
    {/if}

    <section aria-labelledby="recent-heading">
      <div class="section-head">
        <div>
          <h2 id="recent-heading">Recent activity</h2>
          <p>Albums and playback contexts are grouped by the latest listen.</p>
        </div>
      </div>
      {#if loadingRecent}
        <p class="muted">Loading recent activity…</p>
      {:else if recentError}
        <div class="state"><span>{recentError}</span><button onclick={() => reloadKey++}>Retry</button></div>
      {:else if recent.length}
        <div class="grid">
          {#each recent as item (item.uri)}
            <button class="card" onclick={() => playRecent(item)}>
              {#if item.imageUrl}<img src={item.imageUrl} alt="" loading="lazy" />{:else}<span class="art"></span>{/if}
              <span class="eyebrow">{item.kind}</span>
              <span class="truncate title">{item.name}</span>
              <span class="truncate sub">{item.subtitle}</span>
            </button>
          {/each}
        </div>
      {:else}
        <p class="muted">Nothing here yet — <button class="link" onclick={onBrowseLibrary}>browse your library</button>.</p>
      {/if}
    </section>

    <section aria-labelledby="mix-heading">
      <div class="section-head">
        <div><h2 id="mix-heading">Your listening mix</h2><p>Built from your Spotify top tracks, not an editorial Spotify playlist.</p></div>
      </div>
      {#if loadingMix}
        <p class="muted">Loading your listening history…</p>
      {:else if mixError}
        <div class="state"><span>{mixError}</span><button onclick={() => reloadKey++}>Retry</button></div>
      {:else if topTracks.length}
        <div class="grid">
          {#each topTracks as track (track.uri)}
            <button class="card" onclick={() => playMix(track)}>
              {#if track.imageUrl}<img src={track.imageUrl} alt="" loading="lazy" />{:else}<span class="art"></span>{/if}
              <span class="truncate title">{track.name}</span>
              <span class="truncate sub">{track.artists.join(", ")}</span>
            </button>
          {/each}
        </div>
      {:else}<p class="muted">Spotify does not have enough top-listening data for this account yet.</p>{/if}
    </section>

    <section aria-labelledby="discovery-heading">
      <div class="section-head">
        <div><h2 id="discovery-heading">From your top artists</h2><p>Recent releases selected from artists in your Spotify listening history.</p></div>
      </div>
      {#if loadingDiscovery}
        <p class="muted">Finding releases…</p>
      {:else if discoveryError}
        <div class="state"><span>{discoveryError}</span><button onclick={() => reloadKey++}>Retry</button></div>
      {:else if discovery.length}
        <div class="grid">
          {#each discovery as album (album.uri)}
            <button class="card" onclick={() => (openAlbum = album)}>
              {#if album.imageUrl}<img src={album.imageUrl} alt="" loading="lazy" />{:else}<span class="art"></span>{/if}
              <span class="truncate title">{album.name}</span>
              <span class="truncate sub">{album.artists.join(", ")}</span>
            </button>
          {/each}
        </div>
      {:else}<p class="muted">No supported release suggestions are available yet.</p>{/if}
    </section>
  </div>
{/if}

<style>
  .home { padding-bottom: 18px; }
  .greet { font-size: 30px; font-weight: 600; letter-spacing: -0.5px; margin: 18px 0 4px; }
  .greet-sub { margin: 0 0 24px; }
  .greet-sub strong { color: var(--fg); font-weight: 500; }
  .jump { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 10px; }
  .pill { display: flex; align-items: center; gap: 13px; padding: 9px 16px 9px 9px; border-radius: var(--r-md); text-align: left; background: var(--glass); border: 1px solid var(--hairline); backdrop-filter: blur(var(--blur)); transition: background .18s, transform .18s, border-color .18s; }
  .pill:hover { background: var(--glass-hover); border-color: rgba(255,241,224,.16); transform: translateY(-2px); }
  .pill img, .pill .art { width: 46px; height: 46px; border-radius: 9px; object-fit: cover; flex: none; background: rgba(255,241,224,.06); }
  .pill .label { font-weight: 500; min-width: 0; }
  section { margin-top: 28px; }
  .section-head { display: flex; justify-content: space-between; align-items: end; margin-bottom: 12px; }
  h2 { margin: 0 0 3px; font-size: 18px; }
  .section-head p { margin: 0; color: var(--fg-dim); font-size: 12px; }
  .eyebrow { color: var(--accent); font-size: 10px; text-transform: uppercase; letter-spacing: .08em; }
  .state { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 14px 16px; background: var(--glass); border: 1px solid var(--hairline); border-radius: var(--r-md); color: var(--fg-dim); }
  .state button, .link { color: var(--fg); text-decoration: underline; text-underline-offset: 2px; }
  @media (max-width: 900px) { .jump { grid-template-columns: repeat(2, minmax(0,1fr)); } }
  @media (max-width: 600px) { .jump { grid-template-columns: 1fr; } }
</style>
