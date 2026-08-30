<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import { encodeCoverJpeg } from "../images";
  import TrackList from "../components/TrackList.svelte";
  import { trackCountLabel } from "../types";
  import type { PlaylistSummary, TrackSummary } from "../types";

  let {
    playlist,
    onBack,
    onEdited,
  }: {
    playlist: PlaylistSummary;
    onBack: () => void;
    /** Lets the owning list refresh the card the user just came from —
     * `playlist` is a prop, so this view cannot fix a stale name on its own. */
    onEdited?: (playlist: PlaylistSummary) => void;
  } = $props();

  /** The Web API caps playlist tracks at 100 per request, not the usual 50. */
  const PAGE = 100;

  let tracks = $state<TrackSummary[]>([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  /** A short page means the server ran out; nothing more to ask for. */
  let exhausted = $state(false);
  let imageBroken = $state(false);
  // Bumped each time the playlist-load effect below reruns, so a loadMore()
  // still in flight for the previous playlist can detect it's stale once it
  // resolves — otherwise rapid playlist navigation could append the old
  // playlist's next page onto the new playlist's (already-reset) track list.
  let generation = 0;
  $effect(() => {
    playlist.id;
    imageBroken = false;
  });

  type EditableDetails = Pick<PlaylistSummary, "name" | "description" | "imageUrl">;

  // Locally-applied edits, tagged with the playlist they belong to.
  // `playlist` is a prop owned by whichever list opened this view, so a
  // successful rename has to be reflected here rather than waiting for that
  // list to refetch — and the id tag means navigating to a *different*
  // playlist drops them without needing an effect to clear them (which would
  // render the previous playlist's name for one frame first).
  let edited = $state<(EditableDetails & { id: string }) | null>(null);
  const details = $derived<EditableDetails>(
    edited?.id === playlist.id
      ? edited
      : {
          name: playlist.name,
          description: playlist.description,
          imageUrl: playlist.imageUrl,
        },
  );

  /* Spotify answers 403 for a playlist the token cannot modify, which is the
     real check. This only decides whether to *offer* the button, and it
     compares user ids rather than display names because those are not unique.
     A collaborative playlist someone else owns is intentionally excluded: you
     can add tracks to one, but not rename it. */
  const canEdit = $derived(
    Boolean(
      store.auth.userId &&
        playlist.ownerId &&
        playlist.ownerId === store.auth.userId,
    ),
  );

  let editing = $state(false);
  let draftName = $state("");
  let draftDescription = $state("");
  let draftCover = $state<string | null>(null);
  let draftCoverPreview = $state<string | null>(null);
  let editBusy = $state(false);
  let editError = $state<string | null>(null);
  let firstField = $state<HTMLInputElement | null>(null);

  function openEditor() {
    draftName = details.name;
    draftDescription = details.description ?? "";
    draftCover = null;
    draftCoverPreview = null;
    editError = null;
    editing = true;
    queueMicrotask(() => firstField?.select());
  }

  function closeEditor() {
    if (editBusy) return;
    editing = false;
  }

  async function pickCover(event: Event) {
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    editError = null;
    try {
      draftCover = await encodeCoverJpeg(file);
      draftCoverPreview = `data:image/jpeg;base64,${draftCover}`;
    } catch (error) {
      draftCover = draftCoverPreview = null;
      editError = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveEdits() {
    const name = draftName.trim();
    if (!name || editBusy) return;
    editBusy = true;
    editError = null;
    try {
      const nextDescription = draftDescription.trim();
      const changedName = name !== details.name;
      const changedDescription = nextDescription !== (details.description ?? "");
      if (changedName || changedDescription) {
        await api.updatePlaylistDetails(playlist.id, {
          name: changedName ? name : undefined,
          description: changedDescription ? nextDescription : undefined,
        });
      }
      // Second call by necessity: the cover is a separate endpoint with a
      // separate scope, and it can fail on its own after the rename landed.
      if (draftCover) {
        await api.updatePlaylistImage(playlist.id, draftCover);
      }
      const next: EditableDetails = {
        name,
        description: nextDescription || null,
        imageUrl: draftCoverPreview ?? details.imageUrl,
      };
      edited = { ...next, id: playlist.id };
      imageBroken = false;
      onEdited?.({ ...playlist, ...next });
      editing = false;
    } catch (error) {
      editError = store.handleError(error, false).message;
    } finally {
      editBusy = false;
    }
  }

  $effect(() => {
    const id = playlist.id;
    let cancelled = false;
    generation++;
    loading = true;
    (async () => {
      try {
        const t = await api.getPlaylistTracks(id, PAGE, 0);
        if (!cancelled) {
          tracks = t;
          exhausted = t.length < PAGE;
        }
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

  async function loadMore() {
    if (loadingMore || exhausted) return;
    const gen = generation;
    loadingMore = true;
    try {
      const next = await api.getPlaylistTracks(playlist.id, PAGE, tracks.length);
      if (gen !== generation) return; // superseded by a different playlist
      tracks = [...tracks, ...next];
      exhausted = next.length < PAGE;
    } catch (e) {
      if (gen === generation) store.handleError(e);
    } finally {
      if (gen === generation) loadingMore = false;
    }
  }
</script>

<div class="view">
  <div class="head glass">
    <button class="back" onclick={onBack} title="Back">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M15 5l-7 7 7 7" /></svg>
    </button>
    {#if details.imageUrl && !imageBroken}
      <img src={details.imageUrl} alt="" onerror={() => (imageBroken = true)} />
    {:else}
      <span class="ph"></span>
    {/if}
    <span class="meta">
      <span class="truncate name">{details.name}</span>
      <span class="truncate muted">
        {playlist.owner} · {trackCountLabel(playlist.trackCount)}
      </span>
      {#if details.description}
        <span class="truncate muted desc">{details.description}</span>
      {/if}
    </span>
    {#if canEdit}
      <button class="edit" onclick={openEditor} title="Edit playlist details">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" /></svg>
        Edit
      </button>
    {/if}
    <button
      class="btn-primary"
      onclick={() => store.run(() => api.loadContext(playlist.uri))}
      disabled={tracks.length === 0}>Play</button
    >
  </div>

  {#if editing}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="modal-backdrop"
      role="presentation"
      onclick={closeEditor}
      onkeydown={(event) => event.key === "Escape" && closeEditor()}
    >
      <!-- The click handler above closes on backdrop clicks; stopping
           propagation here keeps a click *inside* the dialog from doing the
           same. -->
      <div
        class="modal menu-surface"
        role="dialog"
        aria-modal="true"
        aria-label="Edit playlist"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
        onkeydown={(event) => event.key === "Escape" && closeEditor()}
      >
        <h2>Edit playlist</h2>
        <div class="modal-body">
          <label class="cover">
            {#if draftCoverPreview ?? details.imageUrl}
              <img src={draftCoverPreview ?? details.imageUrl} alt="" />
            {:else}
              <span class="ph"></span>
            {/if}
            <span class="cover-cta">Choose image</span>
            <input type="file" accept="image/*" onchange={pickCover} disabled={editBusy} />
          </label>
          <div class="fields">
            <label>
              <span>Name</span>
              <input bind:this={firstField} bind:value={draftName} maxlength="100" disabled={editBusy} />
            </label>
            <label>
              <span>Description</span>
              <textarea bind:value={draftDescription} maxlength="300" rows="3" disabled={editBusy}></textarea>
            </label>
          </div>
        </div>
        <p class="hint muted">Covers are re-encoded to a square JPEG under Spotify’s 256 KB limit before upload.</p>
        {#if editError}<p class="edit-error" role="alert">{editError}</p>{/if}
        <div class="modal-actions">
          <button class="cancel" onclick={closeEditor} disabled={editBusy}>Cancel</button>
          <button class="btn-primary" onclick={saveEdits} disabled={editBusy || !draftName.trim()}>
            {editBusy ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    <TrackList {tracks} contextUri={playlist.uri} />
    {#if !exhausted}
      <button class="more" onclick={loadMore} disabled={loadingMore}>
        {loadingMore ? "Loading…" : "Load more"}
      </button>
    {/if}
  {/if}
</div>

<style>
  .view {
    padding-top: 18px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 18px;
    border-radius: var(--r-md);
    margin-bottom: 18px;
  }
  .head img,
  .ph {
    width: 84px;
    height: 84px;
    border-radius: 11px;
    object-fit: cover;
    flex: none;
    background: rgba(255, 241, 224, 0.06);
  }
  .meta {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    gap: 2px;
  }
  .name {
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.3px;
  }
  .back {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    color: var(--fg-dim);
    flex: none;
  }
  .back:hover {
    background: var(--glass-hover);
    color: var(--fg);
  }
  .more {
    display: block;
    margin: 20px auto 8px;
    padding: 8px 22px;
    border-radius: 999px;
    color: var(--fg-dim);
    background: var(--glass);
    border: 1px solid var(--hairline);
  }
  .more:hover:not(:disabled) {
    color: var(--fg);
  }
  .desc {
    font-size: 12px;
  }
  .edit {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    flex: none;
    padding: 8px 14px;
    border-radius: 999px;
    color: var(--fg-dim);
    background: var(--glass);
    border: 1px solid var(--hairline);
  }
  .edit:hover {
    color: var(--fg);
    background: var(--glass-hover);
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay-backdrop);
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(9, 6, 3, 0.5);
    backdrop-filter: blur(3px);
  }
  .modal {
    z-index: var(--z-overlay);
    width: min(520px, 100%);
    padding: 20px 22px 18px;
  }
  .modal h2 {
    margin: 0 0 16px;
    font-size: 17px;
  }
  .modal-body {
    display: flex;
    gap: 16px;
  }
  .fields {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 12px;
  }
  .fields label {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .fields span {
    color: var(--fg-dim);
    font-size: 11px;
  }
  .fields input,
  .fields textarea {
    font: inherit;
    color: var(--fg);
    padding: 9px 11px;
    border: 1px solid var(--control-border);
    border-radius: var(--control-radius);
    background: var(--control-bg);
    resize: none;
  }
  .fields input:focus-visible,
  .fields textarea:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  /* The file input is the label's own control, hidden rather than removed so
     it stays keyboard-reachable and the label still activates it. */
  .cover {
    position: relative;
    display: grid;
    place-items: center;
    width: 132px;
    height: 132px;
    flex: none;
    overflow: hidden;
    border-radius: var(--r-md);
    border: 1px solid var(--hairline);
    background: var(--glass);
    cursor: pointer;
  }
  .cover img,
  .cover .ph {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    background: rgba(255, 241, 224, 0.06);
  }
  .cover-cta {
    position: relative;
    padding: 6px 10px;
    border-radius: 999px;
    font-size: 11px;
    background: rgba(12, 8, 4, 0.66);
    opacity: 0;
    transition: opacity var(--motion-fast) var(--ease-standard);
  }
  .cover:hover .cover-cta,
  .cover:focus-within .cover-cta {
    opacity: 1;
  }
  .cover input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .hint {
    margin: 14px 0 0;
    font-size: 11px;
  }
  .edit-error {
    margin: 8px 0 0;
    color: #ffaaa2;
    font-size: 12px;
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
  .cancel {
    padding: 9px 16px;
    border-radius: 999px;
    color: var(--fg-dim);
    border: 1px solid var(--hairline);
  }
  .cancel:hover:not(:disabled) {
    color: var(--fg);
    background: var(--glass-hover);
  }
  @media (max-width: 560px) {
    .modal-body {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
