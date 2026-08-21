<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import * as api from "../../api";
  import { store } from "../../store.svelte";
  import type { FriendActivity, FriendFeed } from "../../types";

  let closeButton: HTMLButtonElement | null = $state(null);
  let returnFocusTo: HTMLElement | null = null;

  let friends = $state<FriendFeed | null>(null);
  let loading = $state(true);
  let now = $state(Date.now());

  const open = $derived(store.settings.friendsPanelOpen);

  // Fetched once here, app-wide, rather than per-view: friend activity used
  // to live inside Home and stopped updating (and stopped being visible at
  // all) the moment you navigated away from it.
  onMount(() => {
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void api
      .getFriendActivity()
      .then((feed) => { if (!disposed) friends = feed; })
      .catch(() => {})
      .finally(() => { if (!disposed) loading = false; });
    void listen<FriendFeed>(api.EVENT_FRIENDS, (event) => {
      friends = event.payload;
      loading = false;
    }).then((stop) => { if (disposed) stop(); else unlisten = stop; });
    const clock = window.setInterval(() => (now = Date.now()), 30_000);
    return () => {
      disposed = true;
      unlisten?.();
      window.clearInterval(clock);
    };
  });

  let brokenImages = $state<Set<string>>(new Set());
  function onArtworkError(url: string | null | undefined) {
    if (!url || brokenImages.has(url)) return;
    brokenImages = new Set(brokenImages).add(url);
  }

  function playFriend(entry: FriendActivity) {
    if (entry.contextUri) store.run(() => api.loadContext(entry.contextUri!, entry.trackUri));
    else store.run(() => api.loadTracks([entry.trackUri], entry.trackUri));
  }

  function friendTime(entry: FriendActivity) {
    const age = Math.max(0, now - entry.timestampMs);
    if (age <= 2 * 60_000) return "Listening now";
    if (age < 60 * 60_000) return `${Math.floor(age / 60_000)} min`;
    if (age < 24 * 60 * 60_000) return `${Math.floor(age / 3_600_000)} hr`;
    return `${Math.floor(age / 86_400_000)} d`;
  }

  function friendIsLive(entry: FriendActivity) {
    const age = now - entry.timestampMs;
    return age >= 0 && age <= 120_000;
  }

  function close() {
    if (!open) return;
    void store.toggleFriendsPanel();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }

  // Moving focus in/out only matters in drawer mode (narrow widths), where
  // the panel is a modal-ish overlay rather than a permanent layout column —
  // but it's harmless to always do, and simpler than detecting which mode is
  // active from script.
  $effect(() => {
    if (open) {
      returnFocusTo = document.activeElement as HTMLElement | null;
      closeButton?.focus();
    } else {
      returnFocusTo?.focus?.();
      returnFocusTo = null;
    }
  });
</script>

{#if open}
  <button
    class="backdrop"
    type="button"
    aria-label="Close Friend Activity"
    onclick={close}
    tabindex="-1"
  ></button>
{/if}

<aside
  class="panel"
  class:open
  aria-label="Friend Activity"
  aria-hidden={!open}
  onkeydown={onKeydown}
>
  <div class="panel-inner">
    <header>
      <h2>Friends</h2>
      <button bind:this={closeButton} class="close" type="button" onclick={close} aria-label="Close Friend Activity" tabindex={open ? 0 : -1}>
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
      </button>
    </header>

    {#if loading || friends?.status === "connecting"}
      <div class="state"><span>Connecting…</span></div>
    {:else if friends?.entries.length}
      {#if friends.status === "stale"}<p class="muted note">Showing cached activity while presence reconnects.</p>{/if}
      <div class="friends">
        {#each friends.entries as entry (entry.userUri)}
          <button class="friend" onclick={() => playFriend(entry)} tabindex={open ? 0 : -1}>
            <span class="avatar">{#if entry.userImageUrl && !brokenImages.has(entry.userImageUrl)}<img src={entry.userImageUrl} alt="" loading="lazy" onerror={() => onArtworkError(entry.userImageUrl)} />{:else}{entry.userName.slice(0, 1).toUpperCase()}{/if}<i class:live={friendIsLive(entry)}></i></span>
            <span class="copy"><strong class="truncate">{entry.userName}</strong><span class="truncate">{entry.trackName}{entry.artistName ? ` · ${entry.artistName}` : ""}</span><small class="truncate">{entry.contextName ?? entry.albumName ?? "Spotify"}</small></span>
            <time>{friendTime(entry)}</time>
          </button>
        {/each}
      </div>
    {:else if friends?.status === "empty"}
      <div class="state"><span>No friends are listening right now.</span></div>
    {:else if friends?.status === "unavailable"}
      <div class="state"><span>Friend activity isn’t available for this account or region.</span></div>
    {:else}
      <div class="state"><span>Unable to refresh friend activity. Retrying…</span></div>
    {/if}
  </div>
</aside>

<style>
  /* Rail (wide): a real layout column next to <main>, pushing content rather
     than covering it — App.svelte gives it flex:none and lets width do the
     open/close animation. Below the breakpoint there isn't enough room left
     for main content once a rail is subtracted (the window's own minWidth is
     780px), so it becomes a fixed overlay drawer with a backdrop instead;
     that switch is pure CSS, no JS viewport branching. */
  .panel {
    flex: none;
    width: 280px;
    overflow: hidden;
    transition: width var(--motion-normal) var(--ease-standard);
  }
  .panel:not(.open) {
    width: 0;
  }
  .panel-inner {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 280px;
    height: 100%;
    padding: 14px 14px 14px 4px;
  }
  .backdrop {
    display: none;
  }

  @media (max-width: 1050px) {
    .panel {
      position: fixed;
      inset: 46px 0 0 auto;
      z-index: var(--z-overlay);
      width: 300px;
      transform: translateX(100%);
      transition: transform var(--motion-normal) var(--ease-standard);
      background: var(--glass-raised);
      border-left: 1px solid var(--hairline);
      backdrop-filter: blur(var(--blur)) saturate(1.4);
      box-shadow: var(--shadow-raised);
    }
    .panel.open {
      width: 300px;
      transform: translateX(0);
    }
    .panel-inner {
      width: 300px;
      padding: 14px;
    }
    .backdrop {
      display: block;
      position: fixed;
      inset: 46px 0 0 0;
      z-index: var(--z-overlay-backdrop);
      padding: 0;
      border-radius: 0;
      background: rgba(9, 6, 3, 0.4);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .panel {
      transition: none;
    }
  }

  header { display: flex; align-items: center; justify-content: space-between; flex: none; }
  h2 { margin: 0; font-size: 15px; }
  .close { padding: 6px; color: var(--fg-dim); border-radius: var(--r-sm); }
  .close:hover, .close:focus-visible { color: var(--fg); background: var(--glass-hover); }

  .note { margin: 0; font-size: 11px; color: var(--fg-dim); }
  .state { padding: 12px; display: flex; flex-direction: column; gap: 6px; font-size: 12px; color: var(--fg-dim); background: var(--glass); border: 1px solid var(--hairline); border-radius: var(--r-md); }

  .friends { display: flex; flex-direction: column; gap: 8px; overflow-y: auto; min-height: 0; }
  .friend { display: flex; align-items: center; gap: 11px; min-width: 0; padding: 11px; text-align: left; border: 1px solid var(--hairline); border-radius: var(--r-md); background: var(--glass); }
  .friend:hover, .friend:focus-visible { background: var(--glass-hover); }
  .avatar { position: relative; display: grid; place-items: center; width: 42px; height: 42px; flex: none; border-radius: 50%; overflow: visible; background: var(--glass-strong); color: var(--fg-dim); }
  .avatar img { width: 100%; height: 100%; border-radius: inherit; object-fit: cover; }
  .avatar i { position: absolute; right: -1px; bottom: -1px; width: 11px; height: 11px; border-radius: 50%; border: 2px solid var(--ink); background: var(--fg-dim); }
  .avatar i.live { background: var(--accent); }
  .copy { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 2px; }
  .copy span, .copy small, time { color: var(--fg-dim); font-size: 11px; }
  time { flex: none; align-self: flex-start; }
</style>
