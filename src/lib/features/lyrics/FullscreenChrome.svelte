<script lang="ts">
  /**
   * Presentational only — deliberately does not own `fullscreen` state or the
   * F11/Escape handling. Those stay in NowPlaying.svelte because they also
   * drive `panel`/timeline visibility elsewhere in that component, and
   * CLAUDE.md documents the drag-region behavior here as hard-won: keeping
   * the state and window-API calls in one place avoids a second source of
   * truth for something this fragile.
   */
  let {
    fullscreen,
    fullscreenChanging,
    statusText,
    onEnterFullscreen,
    onExitFullscreen,
    onClose,
  }: {
    fullscreen: boolean;
    fullscreenChanging: boolean;
    statusText: string;
    onEnterFullscreen: () => void;
    onExitFullscreen: () => void;
    onClose: () => void;
  } = $props();
</script>

<!--
  No drag region while fullscreen. A borderless Tauri window still honours
  `data-tauri-drag-region` when maximised to the whole screen, so the
  fullscreen lyrics view could be dragged out of fullscreen by grabbing its
  header — the window moved, the content stayed sized for the display, and
  the only way back was Esc. `undefined` rather than `false`: Svelte omits
  the attribute for `undefined`, whereas `false` on a non-boolean attribute
  still renders `data-tauri-drag-region="false"`, which Tauri matches on
  presence and would keep honouring.
-->
<header class="topbar" data-tauri-drag-region={fullscreen ? undefined : true}>
  {#if fullscreen}
    <button
      class="exit"
      onclick={onExitFullscreen}
      disabled={fullscreenChanging}
      title="Exit fullscreen (Esc)"
    >
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 3v5H3M16 3v5h5M8 21v-5H3M16 21v-5h5" /></svg>
      Exit fullscreen
    </button>
  {:else}
    <button
      class="exit"
      onclick={onClose}
      title="Close now playing"
    >
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6 6 18M6 6l12 12" /></svg>
      Close
    </button>
  {/if}
  <div class="drag" data-tauri-drag-region={fullscreen ? undefined : true}></div>
  <span>{statusText}</span>
  {#if !fullscreen}
    <button
      class="fullscreen-button"
      onclick={onEnterFullscreen}
      disabled={fullscreenChanging}
      title="Open fullscreen lyrics (F11)"
    >
      <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M8 3H3v5M16 3h5v5M8 21H3v-5M16 21h5v-5" /></svg>
      Fullscreen lyrics
    </button>
  {/if}
</header>

<style>
  .topbar {
    flex: none;
    min-height: 44px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    color: var(--fg-dim);
    font-size: 11px;
  }
  .drag {
    flex: 1;
    align-self: stretch;
  }
  .topbar button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 32px;
    padding: 0 10px;
    border-radius: var(--control-radius);
  }
  .topbar button:hover:not(:disabled) {
    color: var(--fg);
    background: var(--glass-hover);
  }
</style>
