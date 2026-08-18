<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import { formatMs, type TrackSummary } from "../types";

  let {
    tracks,
    contextUri = null,
  }: { tracks: TrackSummary[]; contextUri?: string | null } = $props();

  const playing = $derived(store.playback.track?.uri ?? null);

  /** id -> saved. Populated lazily; absent means "not yet known". */
  let saved = $state<Record<string, boolean>>({});
  let pending = $state<Record<string, boolean>>({});

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
</script>

<div class="list">
  {#each tracks as t, i (t.uri + i)}
    <div class="row" class:active={t.uri === playing}>
      <button
        class="main"
        onclick={() => playTrack(t)}
        title="Play"
      >
        <span class="idx">{i + 1}</span>
        {#if t.imageUrl}
          <img src={t.imageUrl} alt="" loading="lazy" width="36" height="36" />
        {:else}
          <span class="ph"></span>
        {/if}
        <span class="meta">
          <span class="name truncate">{t.name}</span>
          <span class="artist muted truncate">{t.artists.join(", ")}</span>
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
        class="queue"
        title="Add to queue"
        onclick={() => store.run(() => api.addToQueue(t.uri))}>＋</button
      >
    </div>
  {:else}
    <p class="muted empty">Nothing here.</p>
  {/each}
</div>

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
  .heart {
    padding: 6px 8px;
    color: var(--fg-dim);
    opacity: 0;
  }
  .row:hover .queue,
  .row:hover .heart,
  .heart.on,
  .heart:focus-visible,
  .queue:focus-visible {
    opacity: 1;
  }
  .heart.on {
    color: var(--accent);
  }
  .queue:hover,
  .heart:hover {
    color: var(--fg);
  }
  .empty {
    padding: 16px 8px;
  }
</style>
