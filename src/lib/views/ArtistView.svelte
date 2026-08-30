<script lang="ts">
  import * as api from "../api";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import type { AlbumSummary, ArtistSummary, ArtistStats, ConcertEvent, TrackSummary } from "../types";
  let { artist, onBack, onOpenAlbum }: { artist: ArtistSummary; onBack:()=>void; onOpenAlbum:(a:AlbumSummary)=>void } = $props();
  let loadedDetails=$state<ArtistSummary|null>(null); const details=$derived(loadedDetails??artist);
  let tracks=$state<TrackSummary[]>([]); let likedTracks=$state<TrackSummary[]>([]); let albums=$state<AlbumSummary[]>([]);
  let concerts=$state<ConcertEvent[]>([]); let concertsAvailable=$state(true); let stats=$state<ArtistStats|null>(null);
  let loading=$state(true); let loadingMore=$state(false); let error=$state<string|null>(null); let hasMore=$state(false); let followed=$state(false);
  let showAllTracks=$state(false); let releaseFilter=$state("all"); let reloadKey=$state(0);
  // Bumped on every effect run so a loadMore() still in flight for the
  // previous artist can tell it's stale once it resolves — otherwise rapid
  // artist navigation could append the old artist's next album page onto
  // the new artist's (already-reset) album list.
  let generation=0;
  let brokenImages=$state<Set<string>>(new Set());
  function onArtworkError(url:string|null|undefined){if(!url||brokenImages.has(url))return;brokenImages=new Set(brokenImages).add(url)}
  const visibleAlbums=$derived(releaseFilter==="all"?albums:albums.filter((album)=>album.albumType===releaseFilter));
  const listenerLabel=$derived(stats?.monthlyListeners!=null?`${new Intl.NumberFormat().format(stats.monthlyListeners)} monthly listeners`:null);

  // One Pathfinder call covers stats + top tracks + concerts; the rest are
  // independent REST/library lookups. Only the first two are core content —
  // concerts (folded into the same response) and the liked-songs/follow
  // lookups stay non-fatal, same as before.
  $effect(()=>{ const id=artist.id; reloadKey; let cancelled=false; loading=true; error=null; generation++;
    Promise.allSettled([
      api.getArtist(id).then((v)=>{if(!cancelled)loadedDetails=v}),
      api.getArtistOverview(id).then((v)=>{if(!cancelled){tracks=v.topTracks;concerts=v.concerts.events;concertsAvailable=v.concerts.available;stats=v.stats}}),
      api.getArtistAlbums(id,10,0).then((p)=>{if(!cancelled){albums=p.items;hasMore=p.hasMore}}),
      api.getArtistsSaved([id]).then((v)=>{if(!cancelled)followed=v[0]??false}),
      api.getLikedTracksByArtist(id).then((v)=>{if(!cancelled)likedTracks=v}),
    ]).then((results)=>{const failed=results.slice(0,3).find((r)=>r.status==="rejected");if(!cancelled&&failed?.status==="rejected")error=store.handleError(failed.reason,false).message}).finally(()=>{if(!cancelled)loading=false});
    return()=>{cancelled=true};
  });
  async function loadMore(){if(loadingMore||!hasMore)return;const gen=generation;loadingMore=true;try{const p=await api.getArtistAlbums(artist.id,10,albums.length);if(gen!==generation)return;const seen=new Set(albums.map(a=>a.uri));albums=[...albums,...p.items.filter(a=>!seen.has(a.uri))];hasMore=p.hasMore}catch(e){if(gen===generation)error=store.handleError(e,false).message}finally{if(gen===generation)loadingMore=false}}
  async function toggleFollow(){const gen=generation;const next=!followed;followed=next;try{await api.setArtistsSaved([artist.id],next)}catch(e){if(gen===generation)followed=!next;store.handleError(e)}}
  async function shuffle(){await store.run(async()=>{await api.setShuffle(true);await api.loadContext(details.uri)})}
  // The concert card shows the date as a badge (month over day) plus a
  // separate weekday/time line, so these are formatted as parts rather than
  // as one long Intl string. `startDateIso` is optional and has been seen
  // carrying unparseable values, so every getter degrades to a label instead
  // of rendering "Invalid Date".
  function concertDate(event:ConcertEvent){const iso=event.startDateIso;if(!iso)return null;const date=new Date(iso);return Number.isNaN(date.valueOf())?null:date}
  function concertMonth(event:ConcertEvent){const date=concertDate(event);return date?new Intl.DateTimeFormat(undefined,{month:"short"}).format(date):"TBA"}
  function concertDay(event:ConcertEvent){const date=concertDate(event);return date?new Intl.DateTimeFormat(undefined,{day:"numeric"}).format(date):"—"}
  function concertWhen(event:ConcertEvent){const date=concertDate(event);if(!date)return event.startDateIso??"Date to be announced";return new Intl.DateTimeFormat(undefined,{weekday:"long",year:"numeric",hour:"numeric",minute:"2-digit"}).format(date)}
  function concertPlace(event:ConcertEvent){return [event.venue,event.city].filter(Boolean).join(" · ")||"Venue to be announced"}
</script>
<div class="artist">
  <div class="head" style={details.imageUrl?`--artist-image:url(${details.imageUrl})`:""}>
    <button class="back" onclick={onBack}>← Back</button>{#if details.imageUrl && !brokenImages.has(details.imageUrl)}<img class="avatar" src={details.imageUrl} alt="" onerror={() => onArtworkError(details.imageUrl)} />{:else}<span class="avatar ph"></span>{/if}
    <span class="identity"><span class="kind">Artist</span><h1 class="truncate">{details.name}</h1><small>{listenerLabel??(followed?"Following":"")}</small></span>
    <button class="follow" aria-pressed={followed} onclick={toggleFollow}>{followed?"Following":"Follow"}</button><button class="btn-primary" onclick={()=>store.run(()=>api.loadContext(details.uri))}>▶ Play</button><button class="follow" onclick={shuffle}>Shuffle</button>
  </div>
  {#if error}<div class="state"><span>{error}</span><button onclick={()=>reloadKey++}>Retry</button></div>{/if}
  <section><h2>Popular</h2>{#if loading}<p class="muted">Loading tracks…</p>{:else if tracks.length}<TrackList tracks={showAllTracks?tracks:tracks.slice(0,5)} />{#if tracks.length>5}<button class="more" onclick={()=>showAllTracks=!showAllTracks}>{showAllTracks?"Show less":"Show more"}</button>{/if}{:else}<p class="muted">No playable tracks found.</p>{/if}</section>
  {#if likedTracks.length}<section><h2>Liked songs by {details.name}</h2><TrackList tracks={likedTracks}/></section>{/if}
  {#if concerts.length}<section><h2>On tour</h2><div class="concerts">{#each concerts as event (event.uri)}
      <article>
        <time class="datemark" datetime={event.startDateIso??undefined} aria-label={concertWhen(event)}><span class="month">{concertMonth(event)}</span><span class="day">{concertDay(event)}</span></time>
        <div class="concert-copy">
          <strong class="truncate">{concertPlace(event)}</strong>
          <span class="truncate">{event.title}</span>
          <small>{concertWhen(event)}{event.isFestival?" · Festival":""}</small>
        </div>
        {#if event.eventUrl}<button class="tickets" onclick={()=>openUrl(event.eventUrl!)}>Get tickets</button>{/if}
      </article>
    {/each}</div></section>
  {:else if !concertsAvailable && !loading}<p class="concert-note muted">Concerts are unavailable for this artist or region.</p>{/if}
  <section><div class="release-head"><h2>Discography</h2><div class="filters"><button class:on={releaseFilter==="all"} onclick={()=>releaseFilter="all"}>All</button><button class:on={releaseFilter==="album"} onclick={()=>releaseFilter="album"}>Albums</button><button class:on={releaseFilter==="single"} onclick={()=>releaseFilter="single"}>Singles &amp; EPs</button></div></div>
    {#if loading}<p class="muted">Loading releases…</p>{:else if visibleAlbums.length}<div class="grid">{#each visibleAlbums as album (album.uri)}<button class="card" onclick={()=>onOpenAlbum(album)}>{#if album.imageUrl && !brokenImages.has(album.imageUrl)}<img src={album.imageUrl} alt="" loading="lazy" onerror={() => onArtworkError(album.imageUrl)} />{:else}<span class="ph cover"></span>{/if}<span class="truncate title">{album.name}</span><span class="truncate sub">{album.releaseDate??album.artists.join(", ")} · {album.albumType||"release"}</span></button>{/each}</div>{:else}<p class="muted">No matching releases are available.</p>{/if}
    {#if hasMore}<button class="more" disabled={loadingMore} onclick={loadMore}>{loadingMore?"Loading…":"Load more"}</button>{/if}
  </section>
</div>
<style>
  .artist{padding:0 24px 18px}.head{position:relative;display:flex;align-items:center;gap:14px;margin:18px 0 22px;padding:38px 24px 24px;overflow:hidden;border-radius:var(--r-lg);border:1px solid var(--hairline)}.head::before{content:"";position:absolute;inset:0;background:linear-gradient(90deg,rgba(24,15,8,.94),rgba(24,15,8,.55)),var(--artist-image);background-size:cover;background-position:center;filter:saturate(.75);z-index:-1}.avatar{width:112px;height:112px;border-radius:50%;object-fit:cover;box-shadow:0 12px 35px rgba(0,0,0,.35)}.ph{background:rgba(255,241,224,.06)}.identity{display:flex;flex:1;min-width:0;flex-direction:column}.kind{color:var(--accent);font-size:10px;text-transform:uppercase;letter-spacing:.08em}.identity h1{margin:4px 0;font-size:clamp(28px,5vw,58px)}small,.sub{color:var(--fg-dim)}.follow{padding:8px 12px;border:1px solid var(--hairline);background:var(--glass-strong);border-radius:999px}.follow[aria-pressed="true"]{color:var(--accent)}section{margin-top:27px}h2{margin:0 0 6px;font-size:17px}.concerts{display:grid;gap:10px}
  .concerts article{display:grid;grid-template-columns:auto 1fr auto;align-items:center;gap:16px;padding:14px 16px;border:1px solid var(--hairline);border-radius:var(--r-md);background:var(--glass);transition:background var(--motion-normal) var(--ease-standard),border-color var(--motion-normal) var(--ease-standard)}
  .concerts article:hover{background:var(--glass-hover);border-color:var(--control-border-hover)}
  /* Date as a stacked badge rather than a sentence: it is the field people
     scan a tour list by, and a full localised datetime string reads as a
     paragraph at this size. The long form still exists, on the third line and
     as the time element's aria-label, so nothing is lost to screen readers. */
  .datemark{display:grid;place-items:center;gap:1px;width:58px;padding:8px 0;flex:none;border-radius:var(--r-sm);background:color-mix(in srgb,var(--accent) 16%,transparent);border:1px solid color-mix(in srgb,var(--accent) 34%,transparent);color:var(--accent)}
  .datemark .month{font-size:10px;text-transform:uppercase;letter-spacing:.1em}
  .datemark .day{font-size:21px;font-weight:650;line-height:1;font-variant-numeric:tabular-nums}
  .concert-copy{display:flex;min-width:0;flex-direction:column;gap:2px}
  .concert-copy strong{font-size:16px}
  .concert-copy span{color:var(--fg);font-size:13px}
  .concert-copy small{color:var(--fg-dim);font-size:11px}
  .tickets{flex:none;padding:9px 18px;border-radius:999px;font-weight:600;color:var(--ink);background:var(--accent)}
  .tickets:hover{background:var(--accent-hover)}.concert-note{margin:22px 0 0}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:14px}.cover,.card img{display:block;width:100%;aspect-ratio:1;object-fit:cover;border-radius:11px;margin-bottom:10px}.release-head{display:flex;align-items:center;justify-content:space-between;margin-bottom:10px}.filters{display:flex;gap:5px}.filters button{padding:6px 10px;border-radius:999px;background:var(--glass);border:1px solid var(--hairline)}.filters button.on{color:var(--accent);background:var(--glass-strong)}.more{display:block;margin:16px auto 0;padding:8px 14px;text-decoration:underline}.state{display:flex;justify-content:space-between;padding:12px 14px;border:1px solid var(--hairline);background:var(--glass);border-radius:var(--r-md);color:var(--fg-dim)}@media(max-width:760px){.head{flex-wrap:wrap}.identity{order:3;flex-basis:100%}.avatar{width:84px;height:84px}.release-head{align-items:flex-start;flex-direction:column;gap:8px}.concerts article{grid-template-columns:auto 1fr}.tickets{grid-column:1/-1;justify-self:start}}
</style>
