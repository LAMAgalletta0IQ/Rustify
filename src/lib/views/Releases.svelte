<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import SelectMenu, { type SelectOption } from "../ui/SelectMenu.svelte";
  import type { AlbumSummary } from "../types";
  import AlbumView from "./AlbumView.svelte";

  let releases = $state<AlbumSummary[]>([]);
  let saved = $state<Record<string, boolean>>({});
  let cursor = $state<string | null | undefined>(undefined);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let partial = $state<string[]>([]);
  let type = $state("all");
  let range = $state("90");
  let openAlbum = $state<AlbumSummary | null>(null);
  const typeOptions: SelectOption[] = [
    { value: "all", label: "All releases" },
    { value: "album", label: "Albums" },
    { value: "single", label: "Singles and EPs" },
    { value: "compilation", label: "Compilations" },
  ];
  const rangeOptions: SelectOption[] = [
    { value: "30", label: "Last 30 days" },
    { value: "90", label: "Last 90 days" },
    { value: "365", label: "Last year" },
    { value: "all", label: "Any date" },
  ];

  const visible = $derived.by(() => {
    const cutoff = range === "all" ? 0 : Date.now() - Number(range) * 86_400_000;
    return releases.filter((release) => {
      const matchesType = type === "all" || release.albumType === type;
      const date = release.releaseDate ? Date.parse(release.releaseDate) : NaN;
      return matchesType && (!cutoff || !Number.isFinite(date) || date >= cutoff);
    });
  });

  async function loadMore(reset = false) {
    if (loading || (!reset && cursor === null)) return;
    loading = true;
    error = null;
    try {
      const page = await api.getFollowedReleases(reset ? undefined : cursor ?? undefined);
      const current = reset ? [] : releases;
      const seen = new Set(current.map((release) => release.uri));
      const added = page.items.filter((release) => !seen.has(release.uri));
      releases = [...current, ...added].sort((a, b) => (b.releaseDate ?? "").localeCompare(a.releaseDate ?? ""));
      cursor = page.nextArtist;
      partial = [...partial, ...page.partialErrors];
      const ids = added.map((release) => release.id).filter(Boolean);
      if (ids.length) {
        const states = await api.getAlbumsSaved(ids);
        saved = { ...saved, ...Object.fromEntries(ids.map((id, index) => [id, states[index]])) };
      }
    } catch (e) {
      error = store.handleError(e, false).message;
    } finally {
      loading = false;
    }
  }

  async function toggleSaved(release: AlbumSummary) {
    const next = !saved[release.id];
    saved = { ...saved, [release.id]: next };
    try { await api.setAlbumsSaved([release.id], next); }
    catch (e) { saved = { ...saved, [release.id]: !next }; store.handleError(e); }
  }

  $effect(() => { void loadMore(true); });
</script>

{#if openAlbum}
  <AlbumView album={openAlbum} onBack={() => (openAlbum = null)} />
{:else}
  <div class="releases">
    <header><div><span class="eyebrow">Your followed artists</span><h1>Recent releases</h1><p>Albums and singles from the artists you follow, ordered by the dates Spotify provides.</p></div></header>
    <div class="filters" aria-label="Release filters">
      <div class="filter"><span>Type</span><SelectMenu value={type} options={typeOptions} label="Release type" onChange={(value) => (type = value ?? "all")} /></div>
      <div class="filter"><span>Date</span><SelectMenu value={range} options={rangeOptions} label="Release date range" onChange={(value) => (range = value ?? "all")} /></div>
    </div>
    {#if partial.length}<p class="partial" role="status">Some artist catalogs could not be refreshed, so this page may be incomplete. Try again later.</p>{/if}
    {#if visible.length}
      <div class="grid">
        {#each visible as release (release.uri)}
          <article class="card">
            <button class="open" onclick={() => (openAlbum = release)}>
              {#if release.imageUrl}<img src={release.imageUrl} alt="" loading="lazy" />{:else}<span class="art"></span>{/if}
              <span class="eyebrow">{release.albumType || "release"}</span><strong class="truncate">{release.name}</strong><span class="truncate">{release.artists.join(", ")}</span><time>{release.releaseDate ?? "Release date unavailable"}</time>
            </button>
            <div class="actions"><button aria-label={`Play ${release.name}`} onclick={() => store.run(() => api.loadContext(release.uri))}>▶ Play</button><button aria-pressed={saved[release.id] ?? false} onclick={() => toggleSaved(release)}>{saved[release.id] ? "Saved" : "Save"}</button></div>
          </article>
        {/each}
      </div>
    {:else if !loading && !error}<div class="empty"><h2>No matching releases</h2><p>Follow artists on their pages, or broaden the date and type filters.</p></div>{/if}
    {#if error}<div class="state"><span>{error}</span><button onclick={() => loadMore(cursor === undefined)}>Retry</button></div>{/if}
    {#if loading}<p class="muted" role="status">Loading followed-artist releases…</p>{/if}
    {#if cursor !== null && !loading}<button class="more" onclick={() => loadMore()}>Load more artists</button>{/if}
  </div>
{/if}

<style>
  .releases { padding: 22px 0 38px; } header h1 { margin: 4px 0; } header p, .empty p { margin: 0; color: var(--fg-dim); }
  .filters { display: flex; gap: 12px; margin: 22px 0; } .filter { display: flex; align-items: center; gap: 8px; color: var(--fg-dim); }
  .partial, .state, .empty { padding: 14px 16px; border: 1px solid var(--hairline); background: var(--glass); border-radius: var(--r-md); color: var(--fg-dim); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill,minmax(170px,1fr)); gap: 14px; }
  article { padding: 10px; } .open { display: flex; width: 100%; flex-direction: column; align-items: flex-start; text-align: left; gap: 5px; }
  .open img, .open .art { width: 100%; aspect-ratio: 1; border-radius: 10px; object-fit: cover; margin-bottom: 6px; background: rgba(255,241,224,.06); }
  .open span, time { color: var(--fg-dim); font-size: 11px; } .eyebrow { color: var(--accent)!important; text-transform: uppercase; letter-spacing: .07em; }
  .actions { display: flex; gap: 7px; margin-top: 10px; } .actions button, .more { padding: 7px 10px; border: 1px solid var(--hairline); background: var(--glass-strong); border-radius: var(--r-sm); }
  .actions button[aria-pressed="true"] { color: var(--accent); } .more { display: block; margin: 20px auto 0; }
  .state { display: flex; justify-content: space-between; } @media(max-width:600px){.filters{align-items:stretch;flex-direction:column}.filter{justify-content:space-between}}
</style>
