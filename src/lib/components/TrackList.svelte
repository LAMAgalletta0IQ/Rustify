<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import { formatMs, type TrackCredits, type TrackSummary } from "../types";

  let {
    tracks,
    contextUri = null,
  }: { tracks: TrackSummary[]; contextUri?: string | null } = $props();

  const playing = $derived(store.playback.track?.uri ?? null);

  /** id -> saved. Populated lazily; absent means "not yet known". */
  let saved = $state<Record<string, boolean>>({});
  let pending = $state<Record<string, boolean>>({});
  let creditsTrack = $state<TrackSummary | null>(null);
  let credits = $state<TrackCredits | null>(null);
  let creditsLoading = $state(false);
  let creditsError = $state<string | null>(null);
  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    brokenImages = new Set(brokenImages).add(url);
  }

  // Look up saved state for the visible rows whenever the list changes.
  // Spotify caps the generic library endpoint at 40 URIs per request.
  $effect(() => {
    const ids = tracks.map((t) => t.id).filter(Boolean);
    if (ids.length === 0) return;

    let cancelled = false;
    (async () => {
      try {
        for (let i = 0; i < ids.length; i += 40) {
          const chunk = ids.slice(i, i + 40);
          const flags = await api.getTracksSaved(chunk);
          if (cancelled) return;
          const next = { ...saved };
          chunk.forEach((id, n) => (next[id] = flags[n] ?? false));
          saved = next;
        }
      } catch {
        // A failed lookup just leaves hearts in the unknown state; not worth
        // an error banner over.
      }
    })();

    return () => {
      cancelled = true;
    };
  });

  async function toggleSaved(t: TrackSummary) {
    if (!t.id || pending[t.id]) return;
    const next = !saved[t.id];
    saved = { ...saved, [t.id]: next }; // optimistic
    pending = { ...pending, [t.id]: true };
    try {
      await api.setTracksSaved([t.id], next);
    } catch (e) {
      saved = { ...saved, [t.id]: !next }; // roll back
      store.handleError(e);
    } finally {
      pending = { ...pending, [t.id]: false };
    }
  }

  /**
   * With a container context, start it at the clicked track so playback
   * continues through the rest. Without one (search results), fall back to
   * an ad-hoc track list so the remaining rows still queue up.
   */
  function playTrack(t: TrackSummary) {
    store.run(() =>
      contextUri
        ? api.loadContext(contextUri, t.uri)
        : api.loadTracks(
            tracks.map((x) => x.uri),
            t.uri,
          ),
    );
  }

  async function showCredits(track: TrackSummary) {
    creditsTrack = track;
    credits = null;
    creditsError = null;
    creditsLoading = true;
    try {
      credits = await api.getTrackCredits(track.uri);
    } catch (error) {
      creditsError = store.handleError(error, false).message;
    } finally {
      creditsLoading = false;
    }
  }

  function closeCredits() {
    creditsTrack = null;
    credits = null;
    creditsError = null;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (creditsTrack && event.key === "Escape") closeCredits();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="list">
  {#each tracks as t, i (t.uri + i)}
    <div class="row" class:active={t.uri === playing}>
      <button
        class="main"
        onclick={() => playTrack(t)}
        title="Play"
      >
        <span class="idx">{i + 1}</span>
        {#if t.imageUrl && !brokenImages.has(t.imageUrl)}
          <img
            src={t.imageUrl}
            alt=""
            loading="lazy"
            width="36"
            height="36"
            onerror={() => onArtworkError(t.imageUrl)}
          />
        {:else}
          <span class="ph"></span>
        {/if}
        <span class="meta">
          <span class="name truncate">{t.name}</span>
          <span class="artist muted truncate">{#if t.explicit}<span class="explicit" title="Explicit">E</span>{/if}{t.artists.join(", ")}</span>
        </span>
        <!-- Singles name the album after the track, which just prints the
             title twice on the same row. Show it only when it adds something. -->
        <span class="album muted truncate">
          {t.album === t.name ? "" : t.album}
        </span>
        <span class="dur muted">{formatMs(t.durationMs)}</span>
      </button>
      <button
        class="heart"
        class:on={saved[t.id]}
        disabled={!t.id || pending[t.id]}
        aria-pressed={saved[t.id] ?? false}
        title={saved[t.id] ? "Remove from Liked Songs" : "Save to Liked Songs"}
        onclick={() => toggleSaved(t)}>{saved[t.id] ? "♥" : "♡"}</button
      >
      <button
        class="credits"
        title="Show credits"
        aria-label={`Show credits for ${t.name}`}
        onclick={() => showCredits(t)}>ⓘ</button
      >
      <button
        class="queue"
        title="Add to queue"
        onclick={() => store.run(() => api.addToQueue(t.uri))}>＋</button
      >
    </div>
  {:else}
    <p class="muted empty">Nothing here.</p>
  {/each}
</div>

{#if creditsTrack}
  <div class="credits-overlay">
    <div class="credits-modal" role="dialog" aria-modal="true" aria-labelledby="credits-title">
      <header><div><span class="eyebrow">Track credits</span><h2 id="credits-title">{credits?.trackName ?? creditsTrack.name}</h2></div><button class="close" aria-label="Close credits" onclick={closeCredits}>×</button></header>
      {#if creditsLoading}<p class="muted">Loading credits…</p>
      {:else if creditsError}<p class="credit-error">{creditsError}</p>
      {:else if credits?.groups.length}
        <div class="credit-groups">{#each credits.groups as group (group.roleName)}<div class="credit-group"><h3>{group.roleName}</h3>{#each group.contributors as contributor (contributor.name + contributor.artistUri)}<div class="contributor"><button disabled={!contributor.artistUri} onclick={() => contributor.artistUri && api.loadContext(contributor.artistUri)}>{contributor.name}</button>{#if contributor.roles.length}<span>{contributor.roles.join(", ")}</span>{/if}</div>{/each}</div>{/each}</div>
      {:else}<p class="muted">Spotify did not provide detailed credits for this track.</p>{/if}
      {#if credits?.recordLabel}<footer><span class="muted">Source</span><strong>{credits.recordLabel}</strong></footer>{/if}
    </div>
  </div>
{/if}

<style>
  .list {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
    border-radius: var(--r-sm);
  }
  .row:hover {
    background: var(--glass-hover);
  }
  .row.active .name {
    color: var(--accent);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    /* The title column was 2fr, which at full width left a wide void between
       the title text and the album column — titles rarely fill 400px. */
    grid-template-columns: 28px 36px minmax(0, 1.5fr) minmax(0, 1fr) 48px;
    max-width: 1000px;
    align-items: center;
    gap: 12px;
    padding: 6px 8px;
    text-align: left;
  }
  img,
  .ph {
    width: 36px;
    height: 36px;
    border-radius: 5px;
    background: rgba(255, 241, 224, 0.06);
    object-fit: cover;
  }
  .idx {
    font-size: 12px;
    color: var(--fg-dim);
    text-align: right;
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .artist,
  .album,
  .dur {
    font-size: 12px;
  }
  .dur {
    text-align: right;
  }
  .queue,
  .heart,
  .credits {
    padding: 6px 8px;
    color: var(--fg-dim);
    opacity: 0;
  }
  .row:hover .queue,
  .row:hover .heart,
  .row:hover .credits,
  .heart.on,
  .heart:focus-visible,
  .queue:focus-visible {
    opacity: 1;
  }
  .heart.on {
    color: var(--accent);
  }
  .queue:hover,
  .heart:hover,
  .credits:hover {
    color: var(--fg);
  }
  .empty {
    padding: 16px 8px;
  }
  .explicit { display:inline-grid;place-items:center;width:14px;height:14px;margin-right:5px;border-radius:3px;background:rgba(255,241,224,.16);font-size:9px;color:var(--fg); }
  .credits-overlay { position: fixed; inset: 0; z-index: 100; display: grid; place-items: center; padding: 22px; background: rgba(7,5,3,.72); backdrop-filter: blur(8px); }
  .credits-modal { width: min(620px,100%); max-height: min(720px,88vh); overflow: auto; padding: 22px; border: 1px solid var(--hairline); border-radius: var(--r-lg); background: #17110c; box-shadow: 0 24px 90px rgba(0,0,0,.55); }
  .credits-modal header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  .credits-modal h2 { margin: 3px 0 0; font-size: 22px; }
  .eyebrow { color: var(--accent); font-size: 10px; text-transform: uppercase; letter-spacing: .1em; }
  .close { padding: 2px 8px; color: var(--fg-dim); font-size: 24px; }
  .credit-groups { display: grid; gap: 20px; }
  .credit-group h3 { margin: 0 0 8px; font-size: 13px; }
  .contributor { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; padding: 7px 0; border-top: 1px solid var(--hairline); }
  .contributor button { text-align: left; font-weight: 650; }
  .contributor button:not(:disabled):hover { color: var(--accent); }
  .contributor button:disabled { color: var(--fg); cursor: default; }
  .contributor span { color: var(--fg-dim); font-size: 11px; text-align: right; }
  .credit-error { color: #ffb3b3; }
  .credits-modal footer { display: flex; justify-content: space-between; gap: 16px; margin-top: 20px; padding-top: 12px; border-top: 1px solid var(--hairline); font-size: 12px; }
</style>
