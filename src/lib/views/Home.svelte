<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import PlaylistView from "./PlaylistView.svelte";
  import type { PlaylistSummary, TrackSummary } from "../types";

  let { onBrowseLibrary }: { onBrowseLibrary: () => void } = $props();

  /** Home is a landing surface, not a browser: one row of shortcuts only. */
  const PILLS = 6;
  const RECENT = 12;

  let pills = $state<PlaylistSummary[]>([]);
  let recent = $state<TrackSummary[]>([]);
  let loading = $state(true);

  /** Drill-down stacked over the home surface; null shows the surface. */
  let openPlaylist = $state<PlaylistSummary | null>(null);

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

  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        // Two independent shelves — fire together rather than in series.
        const [p, r] = await Promise.all([
          api.getPlaylists(PILLS, 0),
          api.getRecentlyPlayed(RECENT),
        ]);
        if (!cancelled) {
          pills = p;
          recent = r;
        }
      } catch (e) {
        if (!cancelled) store.error = api.asAppError(e).message;
      } finally {
        if (!cancelled) loading = false;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  /**
   * Recently played has no container context, so play the shelf as an explicit
   * track list — that way the rest keeps playing after the clicked track.
   */
  function playRecent(t: TrackSummary) {
    store.run(() => api.loadTracks(recent.map((x) => x.uri), t.uri));
  }
</script>

{#if openPlaylist}
  <PlaylistView playlist={openPlaylist} onBack={() => (openPlaylist = null)} />
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

    {#if loading}
      <p class="muted">Loading…</p>
    {:else}
      {#if pills.length}
        <div class="jump">
          {#each pills as p (p.id)}
            <button class="pill" onclick={() => (openPlaylist = p)}>
              {#if p.imageUrl}
                <img src={p.imageUrl} alt="" loading="lazy" />
              {:else}
                <span class="art"></span>
              {/if}
              <span class="truncate">{p.name}</span>
            </button>
          {/each}
        </div>
      {/if}

      <div class="sec">Recently played</div>
      {#if recent.length}
        <div class="grid">
          {#each recent as t (t.uri)}
            <button class="card" onclick={() => playRecent(t)}>
              {#if t.imageUrl}
                <img src={t.imageUrl} alt="" loading="lazy" />
              {:else}
                <span class="art"></span>
              {/if}
              <span class="truncate title">{t.name}</span>
              <span class="truncate sub">{t.artists.join(", ")}</span>
            </button>
          {/each}
        </div>
      {:else}
        <p class="muted">
          Nothing here yet — <button class="link" onclick={onBrowseLibrary}>
            browse your library
          </button>.
        </p>
      {/if}
    {/if}
  </div>
{/if}

<style>
  .home {
    padding-bottom: 8px;
  }
  .greet {
    font-size: 30px;
    font-weight: 600;
    letter-spacing: -0.5px;
    margin: 18px 0 4px;
  }
  .greet-sub {
    margin: 0 0 26px;
  }
  .greet-sub strong {
    color: var(--fg);
    font-weight: 500;
  }

  .jump {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 9px 16px 9px 9px;
    border-radius: var(--r-md);
    text-align: left;
    background: var(--glass);
    border: 1px solid var(--hairline);
    backdrop-filter: blur(var(--blur));
    transition: background 0.18s, transform 0.18s, border-color 0.18s;
  }
  .pill:hover {
    background: var(--glass-hover);
    border-color: rgba(255, 255, 255, 0.16);
    transform: translateY(-2px);
  }
  .pill img,
  .pill .art {
    width: 46px;
    height: 46px;
    border-radius: 9px;
    object-fit: cover;
    flex: none;
    background: rgba(255, 255, 255, 0.06);
  }
  .pill span {
    font-weight: 500;
  }

  .link {
    padding: 0;
    color: var(--fg);
    text-decoration: underline;
  }

  @media (max-width: 900px) {
    .jump {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
