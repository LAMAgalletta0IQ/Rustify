<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type {
    AlbumSummary,
    ArtistSummary,
    DjSession,
    FriendActivity,
    FriendFeed,
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
  let dj = $state<DjSession | null>(null);
  let friends = $state<FriendFeed | null>(null);
  let friendsLoading = $state(true);
  let friendNow = $state(Date.now());
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

  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void api.getFriendActivity()
      .then((feed) => { if (!disposed) friends = feed; })
      .catch(() => {})
      .finally(() => { if (!disposed) friendsLoading = false; });
    void listen<FriendFeed>(api.EVENT_FRIENDS, (event) => {
      friends = event.payload;
      friendsLoading = false;
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; });
    const clock = window.setInterval(() => friendNow = Date.now(), 30_000);
    return () => { disposed = true; unlisten?.(); window.clearInterval(clock); };
  });

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

  function playFriend(entry: FriendActivity) {
    if (entry.contextUri) store.run(() => api.loadContext(entry.contextUri!, entry.trackUri));
    else store.run(() => api.loadTracks([entry.trackUri], entry.trackUri));
  }

  function friendTime(entry: FriendActivity) {
    const age = Math.max(0, friendNow - entry.timestampMs);
    if (age <= 2 * 60_000) return "Listening now";
    if (age < 60 * 60_000) return `${Math.floor(age / 60_000)} min`;
    if (age < 24 * 60 * 60_000) return `${Math.floor(age / 3_600_000)} hr`;
    return `${Math.floor(age / 86_400_000)} d`;
  }

  function friendIsLive(entry: FriendActivity) {
    const age = friendNow - entry.timestampMs;
    return age >= 0 && age <= 120_000;
  }

  function openPersonalized(item: HomeItem) {
    if (item.kind === "playlist") {
      openPlaylist = {
        uri: item.uri,
        id: item.id,
        name: item.name,
        owner: item.ownerName ?? item.subtitle ?? "Spotify",
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
            {#if item.imageUrl}
              <img src={item.imageUrl} alt="" loading="lazy" />
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
        <div class="dj-copy">
          <span class="eyebrow">dynamic session</span>
          <h2 id="dj-heading">DJ X</h2>
          {#if dj}
            <p>{dj.tracks.length} tracks resolved from Lexicon{dj.narrationResolved ? "; narration synthesis is ready" : dj.tracks.some((track) => track.narrationKinds.length) ? ", with narration metadata" : ""}.</p>
          {:else if djError}
            <p>{djError}</p>
          {:else}
            <p>Spotify’s personalized, continuously refreshed DJ context.</p>
          {/if}
        </div>
        <button class="dj-action" disabled={loadingDj || !dj} onclick={startDj}>
          {loadingDj ? "Checking…" : dj?.active ? "Restart DJ" : "Start DJ"}
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
                    {#if item.imageUrl}<img src={item.imageUrl} alt="" loading="lazy" />{:else}<span class="art"></span>{/if}
                    <span class="eyebrow">{item.madeForUsername ? "made for you" : item.kind}</span>
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
              {#if album.imageUrl}<img src={album.imageUrl} alt="" loading="lazy" />{:else}<span class="art"></span>{/if}
              <span class="truncate title">{album.name}</span>
              <span class="truncate sub">{album.artists.join(", ")}</span>
            </button>
          {/each}
        </div>
      {:else}<p class="muted">No supported release suggestions are available yet.</p>{/if}
    </section>
    <section aria-labelledby="friends-heading">
      <div class="section-head"><div><h2 id="friends-heading">Friend activity</h2><p>Live updates from your Spotify presence feed.</p></div></div>
      {#if friendsLoading || friends?.status === "connecting"}
        <div class="state"><span>Connecting to friend activity…</span></div>
      {:else if friends?.entries.length}
        {#if friends.status === "stale"}<p class="muted">Showing the latest cached activity while presence reconnects.</p>{/if}
        <div class="friends">
          {#each friends.entries as entry (entry.userUri)}
            <button class="friend" onclick={() => playFriend(entry)}>
              <span class="friend-avatar">{#if entry.userImageUrl}<img src={entry.userImageUrl} alt="" loading="lazy" />{:else}{entry.userName.slice(0,1).toUpperCase()}{/if}<i class:live={friendIsLive(entry)}></i></span>
              <span class="friend-copy"><strong class="truncate">{entry.userName}</strong><span class="truncate">{entry.trackName}{entry.artistName?` · ${entry.artistName}`:""}</span><small class="truncate">{entry.contextName??entry.albumName??"Spotify"}</small></span>
              <time>{friendTime(entry)}</time>
            </button>
          {/each}
        </div>
      {:else if friends?.status === "empty"}
        <div class="state"><span>No visible friend listening activity right now.</span></div>
      {:else}
        <div class="state"><span>Friend activity is not available for this account or region. Rustify does not infer or fabricate presence.</span></div>
      {/if}
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
  .dj-card { display: flex; align-items: center; justify-content: space-between; gap: 20px; padding: 20px; border-radius: var(--r-md); border: 1px solid rgba(126, 95, 255, .32); background: linear-gradient(120deg, rgba(72, 38, 150, .32), var(--glass)); }
  .dj-copy h2 { margin: 3px 0 5px; font-size: 22px; }
  .dj-copy p { margin: 0; color: var(--fg-dim); }
  .dj-action { flex: none; padding: 10px 16px; border-radius: 999px; color: #fff; background: rgba(126, 95, 255, .75); }
  .dj-action:disabled { opacity: .45; cursor: default; }
  .state { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 14px 16px; background: var(--glass); border: 1px solid var(--hairline); border-radius: var(--r-md); color: var(--fg-dim); }
  .friends { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: 8px; }
  .friend { display: flex; align-items: center; gap: 11px; min-width: 0; padding: 11px; text-align: left; border: 1px solid var(--hairline); border-radius: var(--r-md); background: var(--glass); }
  .friend:hover { background: var(--glass-hover); }
  .friend-avatar { position: relative; display: grid; place-items: center; width: 42px; height: 42px; flex: none; border-radius: 50%; overflow: visible; background: var(--glass-strong); color: var(--fg-dim); }
  .friend-avatar img { width: 100%; height: 100%; border-radius: inherit; object-fit: cover; }
  .friend-avatar i { position: absolute; right: -1px; bottom: -1px; width: 11px; height: 11px; border-radius: 50%; border: 2px solid #17100a; background: var(--fg-dim); }
  .friend-avatar i.live { background: var(--accent); }
  .friend-copy { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 2px; }
  .friend-copy span,.friend-copy small,.friend time { color: var(--fg-dim); font-size: 11px; }
  .friend time { flex: none; align-self: flex-start; }
  .state button, .link { color: var(--fg); text-decoration: underline; text-underline-offset: 2px; }
  @media (max-width: 900px) { .jump,.friends { grid-template-columns: repeat(2, minmax(0,1fr)); } }
  @media (max-width: 600px) { .jump,.friends { grid-template-columns: 1fr; } }
</style>
