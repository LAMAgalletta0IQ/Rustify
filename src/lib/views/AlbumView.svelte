<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import TrackList from "../components/TrackList.svelte";
  import type { AlbumSummary, TrackSummary } from "../types";

  let { album, onBack }: { album: AlbumSummary; onBack: () => void } = $props();

  let tracks = $state<TrackSummary[]>([]);
  let loading = $state(true);
  let saved = $state<boolean | null>(null);
  let saving = $state(false);
  let imageBroken = $state(false);
  // Bumped each time the album-load effect below reruns. toggleSaved()'s
  // error rollback captures it at click time, so a save/unsave request that
  // fails *after* the user has already navigated to a different album
  // rolls back this component's `saved` for the album that's actually on
  // screen now, not silently flipping it for whichever album is current.
  let generation = 0;
  $effect(() => {
    album.id;
    imageBroken = false;
  });

  $effect(() => {
    const id = album.id;
    let cancelled = false;
    generation++;
    loading = true;
    (async () => {
      try {
        const [tracksResult, savedResult] = await Promise.allSettled([
          api.getAlbumTracks(id),
          api.getAlbumsSaved([id]),
        ]);
        if (cancelled) return;
        if (tracksResult.status === "fulfilled") {
          // `/albums/{id}/tracks` returns simplified track objects with no
          // album object of their own, so each row otherwise has no cover
          // art. The header (`album`) was already fetched by whoever opened
          // this view — reuse it instead of an extra request per track.
          tracks = tracksResult.value.map((t) => ({
            ...t,
            album: t.album || album.name,
            imageUrl: t.imageUrl ?? album.imageUrl,
          }));
        } else throw tracksResult.reason;
        if (savedResult.status === "fulfilled") saved = savedResult.value[0] ?? false;
      } catch (e) {
        if (!cancelled) store.handleError(e);
      } finally {
        if (!cancelled) loading = false;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  async function toggleSaved() {
    if (saving) return;
    const gen = generation;
    const next = !saved;
    saved = next;
    saving = true;
    try {
      await api.setAlbumsSaved([album.id], next);
    } catch (e) {
      if (gen === generation) saved = !next;
      store.handleError(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="album">
  <div class="head">
    <button class="back" onclick={onBack}>← Back</button>
    {#if album.imageUrl && !imageBroken}
      <img src={album.imageUrl} alt="" onerror={() => (imageBroken = true)} />
    {/if}
    <span class="meta">
      <span class="truncate name">{album.name}</span>
      <span class="truncate muted">{album.artists.join(", ")}</span>
    </span>
    <button
      class="btn-primary"
      onclick={() => store.run(() => api.loadContext(album.uri))}>Play</button
    >
    <button
      class="heart"
      class:on={saved}
      onclick={toggleSaved}
      disabled={saving || saved === null}
      aria-pressed={saved ?? false}
      title={saved ? "Remove album from library" : "Save album to library"}
    >
      {saved ? "♥" : "♡"}
    </button>
  </div>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    <TrackList {tracks} contextUri={album.uri} />
  {/if}
</div>

<style>
  .album {
    padding: 20px 24px 8px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 16px;
  }
  .head img {
    width: 84px;
    height: 84px;
    border-radius: 6px;
    object-fit: cover;
  }
  .meta {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: 20px;
    font-weight: 600;
  }
  .back {
    color: var(--fg-dim);
  }
  .heart {
    font-size: 20px;
    color: var(--fg-dim);
    padding: 6px 10px;
  }
  .heart.on {
    color: var(--accent);
  }
</style>
