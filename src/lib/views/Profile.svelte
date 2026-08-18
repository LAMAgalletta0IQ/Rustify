<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type { ArtistSummary, PlaylistSummary, TrackSummary } from "../types";

  let { onBack }: { onBack: () => void } = $props();
  let topArtists = $state<ArtistSummary[]>([]);
  let topTracks = $state<TrackSummary[]>([]);
  let playlists = $state<PlaylistSummary[]>([]);
  let loading = $state(true);
  let partialError = $state<string | null>(null);

  $effect(() => {
    let cancelled = false;
    Promise.allSettled([
      api.getTopArtists(6),
      api.getTopTracks(6),
      api.getPlaylists(6, 0),
    ]).then((results) => {
      if (cancelled) return;
      if (results[0].status === "fulfilled") topArtists = results[0].value;
      if (results[1].status === "fulfilled") topTracks = results[1].value;
      if (results[2].status === "fulfilled") playlists = results[2].value;
      const failure = results.find((result) => result.status === "rejected");
      if (failure?.status === "rejected") {
        partialError = store.handleError(failure.reason, false).message;
      }
      loading = false;
    });
    return () => { cancelled = true; };
  });

  async function signOut() {
    await store.run(() => store.logout());
  }
</script>

<div class="profile">
  <button class="back" onclick={onBack}>← Back</button>
  <header>
    {#if store.auth.avatarUrl}<img src={store.auth.avatarUrl} alt="" />{:else}<span class="avatar"></span>{/if}
    <div><span class="muted label">Profile</span><h1>{store.auth.displayName ?? store.auth.userId ?? "Spotify account"}</h1><p class="muted">{store.auth.userId}{store.auth.product ? ` · ${store.auth.product}` : ""}</p></div>
  </header>

  {#if loading}<p class="muted">Loading account activity…</p>{/if}
  {#if partialError}<p class="notice">Some account content could not be loaded: {partialError}</p>{/if}

  {#if topArtists.length}<section><h2>Top artists</h2><div class="chips">{#each topArtists as artist (artist.uri)}<span>{artist.name}</span>{/each}</div></section>{/if}
  {#if topTracks.length}<section><h2>Top tracks</h2><ol>{#each topTracks as track (track.uri)}<li><span>{track.name}</span><small>{track.artists.join(", ")}</small></li>{/each}</ol></section>{/if}
  {#if playlists.length}<section><h2>Your playlists</h2><p class="muted">Showing {playlists.length} recent library entries.</p><div class="chips">{#each playlists as playlist (playlist.uri)}<span>{playlist.name}</span>{/each}</div></section>{/if}

  <section class="account"><div><h2>Session</h2><p class="muted">Signing out removes stored OAuth tokens but keeps app settings and the Spotify Client ID.</p></div><button class="danger" onclick={signOut}>Sign out</button></section>
</div>

<style>
  .profile { max-width: 820px; margin: 0 auto; padding: 20px 0 38px; }
  .back { color: var(--fg-dim); margin-bottom: 18px; }
  header { display: flex; align-items: center; gap: 20px; margin-bottom: 24px; }
  header img, .avatar { width: 92px; height: 92px; border-radius: 50%; object-fit: cover; flex: none; background: linear-gradient(135deg,#b08046,#6b4226); }
  h1 { margin: 2px 0; font-size: 28px; }
  header p { margin: 0; }
  .label { font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }
  section { margin-top: 14px; padding: 18px 20px; border: 1px solid var(--hairline); background: var(--glass); border-radius: var(--r-md); }
  h2 { margin: 0 0 10px; font-size: 15px; }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .chips span { padding: 7px 10px; border-radius: 999px; background: var(--glass-strong); font-size: 12px; }
  ol { margin: 0; padding: 0; list-style: none; display: grid; grid-template-columns: 1fr 1fr; gap: 8px 18px; }
  li { display: flex; flex-direction: column; min-width: 0; }
  li small { color: var(--fg-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  section p { margin: 0; }
  .notice { padding: 11px 14px; border: 1px solid rgba(220,190,90,.3); border-radius: var(--r-sm); background: rgba(190,160,50,.14); }
  .account { display: flex; align-items: center; justify-content: space-between; gap: 20px; }
  .danger { padding: 8px 13px; border: 1px solid rgba(220,90,100,.4); border-radius: var(--r-sm); color: #ffb3b3; flex: none; }
</style>
