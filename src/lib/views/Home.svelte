<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type {
    AlbumSummary,
    ArtistSummary,
    DjSession,
    HomeFeed,
    HomeItem,
    PlaylistSummary,
    RecentActivityItem,
    TrackSummary,
  } from "../types";
  import AlbumView from "./AlbumView.svelte";
  import ArtistView from "./ArtistView.svelte";
  import PlaylistView from "./PlaylistView.svelte";

  let { onBrowseLibrary, onOpenForYou, onOpenReleases }: { onBrowseLibrary: () => void; onOpenForYou: () => void; onOpenReleases: () => void } = $props();

  const QUICK_LIMIT = 6;
  const RECENT_LIMIT = 12;
  const MIX_LIMIT = 12;

  let playlists = $state<PlaylistSummary[]>([]);
  let recent = $state<RecentActivityItem[]>([]);
  let quickAccess = $state<RecentActivityItem[]>([]);
  let topTracks = $state<TrackSummary[]>([]);
  let discovery = $state<AlbumSummary[]>([]);
  let personalized = $state<HomeFeed | null>(null);
  /** Mirrors `spotify::dj::FALLBACK_REASON`. A session carrying this reason
   * came from the fallback path — Spotify's Lexicon endpoint refused to
   * resolve a dynamic DJ session for this account, so the app plays the public
   * DJ playlist instead. */
  const DJ_FALLBACK_REASON = "lexicon-unavailable-fallback";
  let dj = $state<DjSession | null>(null);
  let loadingRecent = $state(true);
  let loadingMix = $state(true);
  let loadingDiscovery = $state(true);
  let loadingPersonalized = $state(true);
  let loadingDj = $state(true);
  let recentError = $state<string | null>(null);
  let mixError = $state<string | null>(null);
  let discoveryError = $state<string | null>(null);
  let personalizedError = $state<string | null>(null);
  let djError = $state<string | null>(null);
  let reloadKey = $state(0);

  // Tracks image URLs that failed to *load* (wrong field, CSP block, expired
  // link…), as opposed to items with no URL at all — which already fall back
  // to the plain .art placeholder via the {#if item.imageUrl} check below.
  // Without this the WebView's native broken-image icon stays on screen.
  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    console.debug("[home] artwork failed to load", url);
    brokenImages = new Set(brokenImages).add(url);
  }

  let openPlaylist = $state<PlaylistSummary | null>(null);
  let openAlbum = $state<AlbumSummary | null>(null);
  let openArtist = $state<ArtistSummary | null>(null);

  const greeting = $derived.by(() => {
    if (personalized?.greeting) return personalized.greeting;
    const h = new Date().getHours();
    if (h < 5) return "Still up";
    if (h < 12) return "Good morning";
    if (h < 18) return "Good afternoon";
    return "Good evening";
  });

  const deviceLabel = $derived(
    store.playback.isActiveDevice ? "this device" : "another device",
  );

  $effect(() => {
    reloadKey;
    let cancelled = false;
    loadingRecent = true;
    loadingMix = true;
    loadingDiscovery = true;
    loadingPersonalized = true;
    loadingDj = true;
    recentError = mixError = discoveryError = personalizedError = djError = null;

    const djTask = api
      .getDjStatus(true)
      .then((status) => {
        if (!cancelled) dj = status;
      })
      .catch((e) => {
        if (!cancelled) djError = store.handleError(e, false).message;
      })
      .finally(() => {
        if (!cancelled) loadingDj = false;
      });

    const personalizedTask = api
      .getPersonalizedHome(10)
      .then((feed) => {
        if (!cancelled) personalized = feed;
      })
      .catch((e) => {
        if (!cancelled) personalizedError = store.handleError(e, false).message;
      })
      .finally(() => {
        if (!cancelled) loadingPersonalized = false;
      });

    const recentTask = Promise.allSettled([
      api.getPlaylists(50, 0),
      api.getRecentlyPlayed(50),
    ]).then(([playlistResult, recentResult]) => {
      if (cancelled) return;
      if (playlistResult.status === "fulfilled") playlists = playlistResult.value;
      if (recentResult.status === "fulfilled") {
        const hydrated = recentResult.value.map((item) => {
          const playlist = item.kind === "playlist"
            ? playlists.find((candidate) => candidate.uri === item.uri)
            : undefined;
          return playlist ? { ...item, name: playlist.name, subtitle: playlist.owner, imageUrl: playlist.imageUrl } : item;
        });
        recent = hydrated.slice(0, RECENT_LIMIT);
        api.getQuickAccess(hydrated, QUICK_LIMIT).then((items) => {
          if (!cancelled) quickAccess = items;
        }).catch((e) => {
          if (!cancelled) recentError = store.handleError(e, false).message;
        });
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
      .getFollowedReleases()
      .then((page) => {
        if (!cancelled) {
          discovery = page.items.slice(0, 12);
          if (page.partialErrors.length && !discovery.length) discoveryError = "Some followed-artist catalogs could not be refreshed.";
        }
      })
      .catch((e) => {
        if (!cancelled) discoveryError = store.handleError(e, false).message;
      })
      .finally(() => {
        if (!cancelled) loadingDiscovery = false;
      });

    void Promise.all([djTask, personalizedTask, recentTask, mixTask, discoveryTask]);
    return () => {
      cancelled = true;
    };
  });

  function playlistFor(item: RecentActivityItem): PlaylistSummary | null {
    return playlists.find((playlist) => playlist.uri === item.uri) ?? null;
  }

  function openActivity(item: RecentActivityItem) {
    void api.recordRelevance(item, false).catch(() => {});
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
    void api.recordRelevance(item, true).then(() => api.getQuickAccess(recent, QUICK_LIMIT)).then((items) => (quickAccess = items)).catch(() => {});
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

  function openPersonalized(item: HomeItem) {
    if (item.kind === "playlist") {
      openPlaylist = {
        uri: item.uri,
        id: item.id,
        name: item.name,
        owner: item.ownerName ?? item.subtitle ?? "Spotify",
        // The home feed carries no owner id, so editing stays unoffered for
        // playlists opened from here. That is the right default: everything on
        // this shelf is a Spotify-generated mix nobody can rename anyway.
        ownerId: null,
        description: item.description,
        collaborative: false,
        imageUrl: item.imageUrl,
        trackCount: item.totalCount ?? 0,
      };
    } else if (item.kind === "album") {
      openAlbum = {
        uri: item.uri,
        id: item.id,
        name: item.name,
        artists: item.subtitle ? item.subtitle.split(", ") : [],
        imageUrl: item.imageUrl,
      };
    } else if (item.kind === "artist") {
      openArtist = {
        uri: item.uri,
        id: item.id,
        name: item.name,
        imageUrl: item.imageUrl,
      };
    } else {
      store.run(() => api.loadContext(item.uri));
    }
  }

  function personalizationLabel(item: HomeItem) {
    switch (item.personalization) {
      case "dailyMix": return "daily mix";
      case "discoverWeekly": return "discover weekly";
      case "releaseRadar": return "release radar";
      case "daylist": return "daylist";
      case "artistMix": return "artist mix";
      case "topicMix": return "topic mix";
      case "inspiredByMix": return "inspired mix";
      case "madeForYou": return "made for you";
      default: return item.kind;
    }
  }

  function startDj() {
    loadingDj = true;
    djError = null;
    api.startDj()
      .then((session) => (dj = session))
      .catch((e) => (djError = store.handleError(e, false).message))
      .finally(() => (loadingDj = false));
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
            {#if item.imageUrl && !brokenImages.has(item.imageUrl)}
              <img src={item.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(item.imageUrl)} />
            {:else}
              <span class="art"></span>
            {/if}
            <span class="truncate label">{item.name}</span>
          </button>
        {/each}
      </div>
    {:else if !loadingRecent && !recentError}
      <div class="state"><span>Quick access will learn from real listening and navigation activity on this account.</span><button onclick={onBrowseLibrary}>Browse library</button></div>
    {/if}

    <section aria-labelledby="dj-heading">
      <div class="dj-card">
        <span class="dj-mark" aria-hidden="true">
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
            <path d="M4 13a8 8 0 0 1 16 0" />
            <rect x="2.5" y="13" width="4" height="7" rx="1.5" />
            <rect x="17.5" y="13" width="4" height="7" rx="1.5" />
          </svg>
        </span>
        <div class="dj-copy">
          <h2 id="dj-heading">DJ</h2>
          {#if dj?.active && store.playback.track}
            <p class="truncate">Now playing · {store.playback.track.name}</p>
            {#if dj.reason === DJ_FALLBACK_REASON}
              <p class="dj-note">Playing Spotify’s DJ playlist. The personalized mix and its voice intros are restricted to Spotify’s own clients.</p>
            {/if}
          {:else if dj}
            <p>Your personalized mix, picked and introduced for you.</p>
            {#if dj.reason === DJ_FALLBACK_REASON}
              <p class="dj-note">Spotify restricts the personalized DJ mix and its AI voice intros to its own clients, so this plays the public DJ playlist instead.</p>
            {:else if dj.narrationResolved && !dj.narrationPlaybackSupported}
              <p class="dj-note">Voice intros aren’t available in this app yet — the music still plays.</p>
            {/if}
          {:else if djError}
            <p>{djError}</p>
          {:else}
            <p>Your personalized mix, picked and introduced for you.</p>
          {/if}
        </div>
        <button class="btn-primary dj-action" disabled={loadingDj || !dj} onclick={startDj}>
          {loadingDj
            ? "Checking…"
            : dj?.active
              ? dj.interactivityEnabled && dj.jumpButtonLabel
                ? dj.jumpButtonLabel
                : "Restart"
              : "Play"}
        </button>
      </div>
    </section>

    <section aria-labelledby="spotify-home-heading">
      <div class="section-head">
        <div>
          <h2 id="spotify-home-heading">Made for you on Spotify</h2>
          <p>Daily Mixes, weekly recommendations, daylist and personalized shelves from your account.</p>
        </div>
      </div>
      {#if loadingPersonalized}
        <p class="muted">Loading Spotify Home…</p>
      {:else if personalizedError}
        <div class="state"><span>{personalizedError}</span><button onclick={() => reloadKey++}>Retry</button></div>
      {:else if personalized?.sections.length}
        <div class="personalized-sections">
          {#each personalized.sections as section (section.uri || section.title)}
            <div class="shelf">
              <h3>{section.title ?? "Recommended"}</h3>
              <div class="grid">
                {#each section.items as item (item.uri)}
                  <button class="card" onclick={() => openPersonalized(item)}>
                    {#if item.imageUrl && !brokenImages.has(item.imageUrl)}<img src={item.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(item.imageUrl)} />{:else}<span class="art"></span>{/if}
                    <span class="eyebrow">{personalizationLabel(item)}</span>
                    <span class="truncate title">{item.name}</span>
                    <span class="truncate sub">{item.subtitle ?? item.description ?? "Spotify"}</span>
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="muted">Spotify did not return personalized Home shelves for this account or region.</p>
      {/if}
    </section>

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
              {#if item.imageUrl && !brokenImages.has(item.imageUrl)}<img src={item.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(item.imageUrl)} />{:else}<span class="art"></span>{/if}
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
        <div><h2 id="mix-heading">For you</h2><p>Built from your Spotify top tracks, not an editorial Spotify playlist.</p></div><button class="link" onclick={onOpenForYou}>Show all</button>
      </div>
      {#if loadingMix}
        <p class="muted">Loading your listening history…</p>
      {:else if mixError}
        <div class="state"><span>{mixError}</span><button onclick={() => reloadKey++}>Retry</button></div>
      {:else if topTracks.length}
        <div class="grid">
          {#each topTracks as track (track.uri)}
            <button class="card" onclick={() => playMix(track)}>
              {#if track.imageUrl && !brokenImages.has(track.imageUrl)}<img src={track.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(track.imageUrl)} />{:else}<span class="art"></span>{/if}
              <span class="truncate title">{track.name}</span>
              <span class="truncate sub">{track.artists.join(", ")}</span>
            </button>
          {/each}
        </div>
      {:else}<p class="muted">Spotify does not have enough top-listening data for this account yet.</p>{/if}
    </section>

    <section aria-labelledby="discovery-heading">
      <div class="section-head">
        <div><h2 id="discovery-heading">New from followed artists</h2><p>Actual dated releases from artists you follow.</p></div><button class="link" onclick={onOpenReleases}>Show all</button>
      </div>
      {#if loadingDiscovery}
        <p class="muted">Finding releases…</p>
      {:else if discoveryError}
        <div class="state"><span>{discoveryError}</span><button onclick={() => reloadKey++}>Retry</button></div>
      {:else if discovery.length}
        <div class="grid">
          {#each discovery as album (album.uri)}
            <button class="card" onclick={() => (openAlbum = album)}>
              {#if album.imageUrl && !brokenImages.has(album.imageUrl)}<img src={album.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(album.imageUrl)} />{:else}<span class="art"></span>{/if}
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
  .personalized-sections { display: grid; gap: 22px; }
  .shelf h3 { margin: 0 0 10px; font-size: 15px; font-weight: 600; }
  .dj-card { display: flex; align-items: center; gap: 16px; padding: 18px 20px; border-radius: var(--r-md); border: 1px solid var(--hairline); background: var(--glass); }
  .dj-mark { display: grid; place-items: center; flex: none; width: 44px; height: 44px; border-radius: 50%; color: var(--accent); background: var(--glass-raised); border: 1px solid var(--hairline); }
  .dj-copy { flex: 1; min-width: 0; }
  .dj-copy h2 { margin: 0 0 3px; font-size: 17px; }
  .dj-copy p { margin: 0; color: var(--fg-dim); font-size: 13px; }
  .dj-copy p.dj-note { margin-top: 2px; font-size: 11px; color: var(--fg-faint); }
  .dj-action { flex: none; }
  .state { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 14px 16px; background: var(--glass); border: 1px solid var(--hairline); border-radius: var(--r-md); color: var(--fg-dim); }
  .state button, .link { color: var(--fg); text-decoration: underline; text-underline-offset: 2px; }
  @media (max-width: 900px) { .jump { grid-template-columns: repeat(2, minmax(0,1fr)); } }
  @media (max-width: 600px) { .jump { grid-template-columns: 1fr; } }
</style>
