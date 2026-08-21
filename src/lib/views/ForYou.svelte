<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type { ArtistSummary, TrackSummary } from "../types";
  import ArtistView from "./ArtistView.svelte";
  let tracks = $state<TrackSummary[]>([]); let artists = $state<ArtistSummary[]>([]);
  let openArtist = $state<ArtistSummary | null>(null); let loading = $state(true); let error = $state<string | null>(null);
  // Same failure mode as Home's cards: a URL that fails to *load* (CSP block,
  // expired link…) must fall back to the placeholder, not show the WebView's
  // native broken-image icon.
  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    brokenImages = new Set(brokenImages).add(url);
  }
  $effect(() => { Promise.all([api.getTopTracks(40), api.getTopArtists(20)]).then(([t,a]) => { tracks=t; artists=a; }).catch((e) => (error=store.handleError(e,false).message)).finally(() => (loading=false)); });
</script>
{#if openArtist}<ArtistView artist={openArtist} onBack={() => (openArtist=null)} onOpenAlbum={() => {}} />{:else}
<div class="for-you"><header><span>Personalized locally</span><h1>For you</h1><p>Real Spotify listening signals arranged by Rustify. These are not presented as official Spotify mixes or editorial playlists.</p></header>
{#if loading}<p class="muted">Reading your listening profile…</p>{:else if error}<p class="state">{error}</p>{:else if !tracks.length && !artists.length}<div class="state">Listen for a while and this page will fill with your actual top music.</div>{:else}
{#if tracks.length}<section><h2>Your long-term rotation</h2><div class="grid">{#each tracks as track}<button class="card" onclick={() => store.run(() => api.loadTracks(tracks.map((item) => item.uri), track.uri))}>{#if track.imageUrl && !brokenImages.has(track.imageUrl)}<img src={track.imageUrl} alt="" onerror={() => onArtworkError(track.imageUrl)} />{:else}<span class="art"></span>{/if}<strong class="truncate">{track.name}</strong><span class="truncate">{track.artists.join(", ")}</span></button>{/each}</div></section>{/if}
{#if artists.length}<section><h2>Artists you return to</h2><div class="grid artists">{#each artists as artist}<button class="card" onclick={() => (openArtist=artist)}>{#if artist.imageUrl && !brokenImages.has(artist.imageUrl)}<img src={artist.imageUrl} alt="" onerror={() => onArtworkError(artist.imageUrl)} />{:else}<span class="art"></span>{/if}<strong class="truncate">{artist.name}</strong><span>Artist</span></button>{/each}</div></section>{/if}{/if}</div>{/if}
<style>.for-you{padding:22px 0 38px}header span{color:var(--accent);font-size:10px;text-transform:uppercase;letter-spacing:.08em}h1{margin:5px 0}header p,.card span{color:var(--fg-dim)}header p{margin:0;max-width:680px}section{margin-top:28px}h2{font-size:18px}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:14px}.card{display:flex;flex-direction:column;align-items:flex-start;text-align:left;min-width:0}.card img,.art{width:100%;aspect-ratio:1;object-fit:cover;border-radius:11px;margin-bottom:10px;background:rgba(255,241,224,.06)}.artists img,.artists .art{border-radius:50%}.state{padding:16px;margin-top:20px;border:1px solid var(--hairline);background:var(--glass);border-radius:var(--r-md);color:var(--fg-dim)}</style>
