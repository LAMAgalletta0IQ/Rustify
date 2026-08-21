<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type { ArtistSummary, PlaylistSummary, TrackSummary, UserProfile } from "../types";

  let { onBack }: { onBack: () => void } = $props();
  let topArtists = $state<ArtistSummary[]>([]);
  let topTracks = $state<TrackSummary[]>([]);
  let playlists = $state<PlaylistSummary[]>([]);
  let profile = $state<UserProfile | null>(null);
  let profileLoading = $state(true);
  let profileError = $state<string | null>(null);
  let profileQuery = $state("");
  let loading = $state(true);
  let partialError = $state<string | null>(null);

  $effect(() => {
    let cancelled = false;
    Promise.allSettled([
      api.getTopArtists(6),
      api.getTopTracks(6),
      api.getPlaylists(6, 0),
      api.getUserProfile(),
    ]).then((results) => {
      if (cancelled) return;
      if (results[0].status === "fulfilled") topArtists = results[0].value;
      if (results[1].status === "fulfilled") topTracks = results[1].value;
      if (results[2].status === "fulfilled") playlists = results[2].value;
      if (results[3].status === "fulfilled") profile = results[3].value;
      else if (results[3].status === "rejected") profileError = store.handleError(results[3].reason, false).message;
      const failure = results.slice(0, 3).find((result) => result.status === "rejected");
      if (failure?.status === "rejected") partialError = store.handleError(failure.reason, false).message;
      loading = false;
      profileLoading = false;
    });
    return () => { cancelled = true; };
  });

  async function signOut() {
    await store.run(() => store.logout());
  }

  async function loadProfile(username?: string) {
    profileLoading = true;
    profileError = null;
    try {
      profile = await api.getUserProfile(username);
      profileQuery = "";
    } catch (error) {
      profileError = store.handleError(error, false).message;
    } finally {
      profileLoading = false;
    }
  }

  function submitProfile(event: SubmitEvent) {
    event.preventDefault();
    const query = profileQuery.trim();
    if (query) void loadProfile(query);
  }

  function profileColor() {
    if (profile?.color == null) return undefined;
    const rgb = (profile.color >>> 0) & 0xffffff;
    return `#${rgb.toString(16).padStart(6, "0")}55`;
  }
</script>

<div class="profile">
  <button class="back" onclick={onBack}>← Back</button>
  <form class="profile-search" onsubmit={submitProfile}>
    <label for="profile-user">Open a Spotify profile</label>
    <div><input id="profile-user" bind:value={profileQuery} placeholder="Username, spotify:user: URI, or profile URL" /><button disabled={!profileQuery.trim() || profileLoading}>Open</button></div>
  </form>

  <header style:background={profileColor()}>
    {#if profile?.imageUrl ?? store.auth.avatarUrl}<img src={profile?.imageUrl ?? store.auth.avatarUrl ?? undefined} alt="" />{:else}<span class="avatar"></span>{/if}
    <div><span class="muted label">{profile?.isCurrentUser === false ? "Public profile" : "Your profile"}</span><h1>{profile?.displayName ?? store.auth.displayName ?? store.auth.userId ?? "Spotify account"}</h1><p class="muted">{profile?.username ?? store.auth.userId}{profile?.isCurrentUser !== false && store.auth.product ? ` · ${store.auth.product}` : ""}</p></div>
    {#if profile?.isCurrentUser === false}<button class="mine" onclick={() => loadProfile()}>My profile</button>{/if}
  </header>

  {#if loading || profileLoading}<p class="muted">Loading profile…</p>{/if}
  {#if profileError}<p class="notice">Profile could not be loaded: {profileError}</p>{/if}
  {#if partialError}<p class="notice">Some account content could not be loaded: {partialError}</p>{/if}

  {#if profile}
    <section class="stats" aria-label="Profile totals">
      <div><strong>{profile.followingCount ?? profile.following.length}</strong><span>Following</span></div>
      <div><strong>{profile.followers.length || "—"}</strong><span>Visible followers</span></div>
      <div><strong>{profile.totalPublicPlaylistsCount ?? profile.publicPlaylists.length}</strong><span>Public playlists</span></div>
    </section>
    {#if profile.recentlyPlayedArtists.length}<section><h2>Recently played artists</h2><div class="tiles">{#each profile.recentlyPlayedArtists as artist (artist.uri)}<button onclick={() => api.loadContext(artist.uri)}>{#if artist.imageUrl}<img src={artist.imageUrl} alt="" loading="lazy" />{:else}<span class="tile-image"></span>{/if}<strong class="truncate">{artist.name}</strong></button>{/each}</div></section>{/if}
    {#if profile.publicPlaylists.length}<section><h2>Public playlists</h2><div class="tiles">{#each profile.publicPlaylists as playlist (playlist.uri)}<button onclick={() => api.loadContext(playlist.uri)}>{#if playlist.imageUrl}<img src={playlist.imageUrl} alt="" loading="lazy" />{:else}<span class="tile-image square"></span>{/if}<strong class="truncate">{playlist.name}</strong><small class="truncate">{playlist.ownerName ?? profile.displayName}</small></button>{/each}</div></section>{/if}
    {#if profile.following.length}<section><h2>Following</h2><div class="chips">{#each profile.following as item (item.uri)}<button onclick={() => item.uri.startsWith("spotify:artist:") && api.loadContext(item.uri)} disabled={!item.uri.startsWith("spotify:artist:")}>{item.name}</button>{/each}</div></section>{/if}
    {#if profile.showFollows && profile.followers.length}<section><h2>Followers</h2><div class="chips">{#each profile.followers as item (item.uri)}<button disabled>{item.name}</button>{/each}</div></section>{/if}
  {/if}

  {#if profile?.isCurrentUser !== false}
    {#if topArtists.length}<section><h2>Your top artists</h2><div class="chips">{#each topArtists as artist (artist.uri)}<button onclick={() => api.loadContext(artist.uri)}>{artist.name}</button>{/each}</div></section>{/if}
    {#if topTracks.length}<section><h2>Your top tracks</h2><ol>{#each topTracks as track (track.uri)}<li><button onclick={() => api.loadTracks([track.uri], track.uri)}><span>{track.name}</span><small>{track.artists.join(", ")}</small></button></li>{/each}</ol></section>{/if}
    {#if playlists.length}<section><h2>Your library playlists</h2><p class="muted">Showing {playlists.length} recent library entries.</p><div class="chips">{#each playlists as playlist (playlist.uri)}<button onclick={() => api.loadContext(playlist.uri)}>{playlist.name}</button>{/each}</div></section>{/if}
  {/if}

  {#if profile?.isCurrentUser !== false}<section class="account"><div><h2>Session</h2><p class="muted">Signing out removes stored OAuth tokens but keeps app settings and the Spotify Client ID.</p></div><button class="danger" onclick={signOut}>Sign out</button></section>{/if}
</div>

<style>
  .profile { max-width: 820px; margin: 0 auto; padding: 20px 0 38px; }
  .back { color: var(--fg-dim); margin-bottom: 18px; }
  header { display: flex; align-items: center; gap: 20px; margin-bottom: 24px; padding: 18px; border-radius: var(--r-md); }
  header img, .avatar { width: 92px; height: 92px; border-radius: 50%; object-fit: cover; flex: none; background: linear-gradient(135deg,#b08046,#6b4226); }
  h1 { margin: 2px 0; font-size: 28px; }
  header p { margin: 0; }
  .label { font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }
  .mine { margin-left: auto; padding: 8px 12px; border: 1px solid var(--hairline); border-radius: var(--r-sm); }
  .profile-search { display: grid; gap: 6px; margin-bottom: 14px; }
  .profile-search label { color: var(--fg-dim); font-size: 11px; }
  .profile-search div { display: flex; gap: 8px; }
  .profile-search input { flex: 1; min-width: 0; padding: 9px 11px; border: 1px solid var(--hairline); border-radius: var(--r-sm); background: var(--glass); color: var(--fg); }
  .profile-search button { padding: 8px 14px; border-radius: var(--r-sm); background: var(--accent); color: #14100d; font-weight: 700; }
  .profile-search button:disabled { opacity: .45; }
  section { margin-top: 14px; padding: 18px 20px; border: 1px solid var(--hairline); background: var(--glass); border-radius: var(--r-md); }
  h2 { margin: 0 0 10px; font-size: 15px; }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .chips button { padding: 7px 10px; border-radius: 999px; background: var(--glass-strong); font-size: 12px; }
  .chips button:disabled { cursor: default; }
  .stats { display: grid; grid-template-columns: repeat(3,1fr); gap: 12px; }
  .stats div { display: flex; flex-direction: column; }
  .stats strong { font-size: 22px; }
  .stats span { color: var(--fg-dim); font-size: 11px; }
  .tiles { display: grid; grid-template-columns: repeat(5,minmax(0,1fr)); gap: 10px; }
  .tiles button { display: flex; min-width: 0; flex-direction: column; gap: 5px; text-align: left; }
  .tiles img,.tile-image { width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 50%; background: var(--glass-strong); }
  .tiles .square { border-radius: var(--r-sm); }
  .tiles small { color: var(--fg-dim); }
  ol { margin: 0; padding: 0; list-style: none; display: grid; grid-template-columns: 1fr 1fr; gap: 8px 18px; }
  li,li button { display: flex; flex-direction: column; min-width: 0; text-align: left; }
  li small { color: var(--fg-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  section p { margin: 0; }
  .notice { padding: 11px 14px; border: 1px solid rgba(220,190,90,.3); border-radius: var(--r-sm); background: rgba(190,160,50,.14); }
  .account { display: flex; align-items: center; justify-content: space-between; gap: 20px; }
  .danger { padding: 8px 13px; border: 1px solid rgba(220,90,100,.4); border-radius: var(--r-sm); color: #ffb3b3; flex: none; }
  @media (max-width: 700px) { .tiles { grid-template-columns: repeat(3,minmax(0,1fr)); } .stats { grid-template-columns: 1fr; } }
</style>
