<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import * as api from "./lib/api";
  import { store } from "./lib/store.svelte";
  import PlayerBar from "./lib/components/PlayerBar.svelte";
  import Home from "./lib/views/Home.svelte";
  import Library from "./lib/views/Library.svelte";
  import Login from "./lib/views/Login.svelte";
  import NowPlaying from "./lib/views/NowPlaying.svelte";
  import Search from "./lib/views/Search.svelte";

  type Tab = "home" | "search" | "library";

  let tab = $state<Tab>("home");
  let nowPlayingOpen = $state(false);
  let main: HTMLElement | null = $state(null);

  const appWindow = getCurrentWindow();

  function go(next: Tab) {
    tab = next;
    nowPlayingOpen = false;
    // Each tab keeps its own component state, but they share one scroller.
    if (main) main.scrollTop = 0;
  }

  onMount(() => store.init());
  onDestroy(() => store.destroy());
</script>

<div class="root">
  <!-- Colour wash behind every panel. Without something to refract, glass
       reads as flat grey. -->
  <div class="ambient"></div>
  <div class="veil"></div>

  {#if store.booting}
    <div class="boot"><p class="muted">Starting…</p></div>
  {:else if !store.auth.loggedIn}
    <!-- The title bar is gone with decorations, so the login screen needs its
         own drag strip or the window becomes unmovable before sign-in. -->
    <div class="predrag" data-tauri-drag-region>
      <div class="wctl">
        <button onclick={() => appWindow.minimize()} title="Minimize">&#9472;</button>
        <button onclick={() => appWindow.toggleMaximize()} title="Maximize">&#9723;</button>
        <button class="x" onclick={() => appWindow.close()} title="Close">&#10005;</button>
      </div>
    </div>
    <Login />
  {:else}
    <div class="shell">
      <header class="titlebar" data-tauri-drag-region>
        <div class="mark" data-tauri-drag-region>
          <span class="dot"></span> Rustify
        </div>

        <nav class="tabs">
          <button class:on={tab === "home"} onclick={() => go("home")}>Home</button>
          <button class:on={tab === "search"} onclick={() => go("search")}>
            Search
          </button>
          <button class:on={tab === "library"} onclick={() => go("library")}>
            Library
          </button>
        </nav>

        <div class="spacer" data-tauri-drag-region></div>

        <button class="who" onclick={() => store.run(api.logout)} title="Log out">
          {#if store.auth.avatarUrl}
            <img src={store.auth.avatarUrl} alt="" width="24" height="24" />
          {:else}
            <span class="av"></span>
          {/if}
          <span class="truncate">{store.auth.displayName ?? "Account"}</span>
        </button>

        <div class="wctl">
          <button onclick={() => appWindow.minimize()} title="Minimize">&#9472;</button>
          <button onclick={() => appWindow.toggleMaximize()} title="Maximize">
            &#9723;
          </button>
          <button class="x" onclick={() => appWindow.close()} title="Close">
            &#10005;
          </button>
        </div>
      </header>

      {#if store.error}
        <div class="banner">
          <span class="truncate">{store.error}</span>
          <button onclick={() => (store.error = null)}>✕</button>
        </div>
      {/if}

      <main bind:this={main}>
        {#if nowPlayingOpen}
          <NowPlaying onClose={() => (nowPlayingOpen = false)} />
        {:else if tab === "home"}
          <Home onBrowseLibrary={() => go("library")} />
        {:else if tab === "search"}
          <Search />
        {:else}
          <Library />
        {/if}
      </main>

      <PlayerBar onOpenNowPlaying={() => (nowPlayingOpen = !nowPlayingOpen)} />
    </div>
  {/if}
</div>

<style>
  .root {
    position: relative;
    height: 100%;
    border-radius: var(--r-lg);
    overflow: hidden;
    isolation: isolate;
  }

  /* Three drifting blobs, deliberately slow: at 26s the movement is felt
     rather than watched. */
  .ambient {
    position: absolute;
    inset: -30%;
    z-index: -2;
    background:
      radial-gradient(42% 46% at 20% 16%, #3d9265 0%, transparent 66%),
      radial-gradient(38% 42% at 84% 24%, #4a51a0 0%, transparent 66%),
      radial-gradient(50% 48% at 60% 90%, #8f3d6e 0%, transparent 64%),
      #0c0c10;
    filter: saturate(1.3);
    animation: drift 26s ease-in-out infinite alternate;
  }
  @keyframes drift {
    from {
      transform: translate3d(-2%, -1%, 0) scale(1.05);
    }
    to {
      transform: translate3d(3%, 2%, 0) scale(1.14);
    }
  }
  /* Respect the OS setting — a permanently moving background is a real
     problem for motion-sensitive users. */
  @media (prefers-reduced-motion: reduce) {
    .ambient {
      animation: none;
    }
  }

  /* Darkening veil plus fine grain: keeps text legible over the blobs and
     stops the gradients from banding. */
  .veil {
    position: absolute;
    inset: 0;
    z-index: -1;
    pointer-events: none;
    /* Balancing act: dark enough that white text stays legible over the blobs,
       light enough that the colour survives a screen full of blurred cards.
       At .55/.78 the content views went flat black. */
    background: linear-gradient(180deg, rgba(6, 6, 9, 0.34), rgba(6, 6, 9, 0.66));
  }
  .veil::after {
    content: "";
    position: absolute;
    inset: 0;
    opacity: 0.16;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.9' numOctaves='3'/%3E%3C/filter%3E%3Crect width='120' height='120' filter='url(%23n)'/%3E%3C/svg%3E");
  }

  .boot {
    display: grid;
    place-items: center;
    height: 100%;
  }
  .predrag {
    display: flex;
    justify-content: flex-end;
    height: 46px;
  }

  .shell {
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    height: 100%;
  }

  .titlebar {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 46px;
    padding: 0 0 0 18px;
  }
  .mark {
    display: flex;
    align-items: center;
    gap: 9px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }
  .mark .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 12px var(--accent);
  }

  .tabs {
    display: flex;
    gap: 2px;
    padding: 3px;
    background: var(--glass);
    border: 1px solid var(--hairline);
    border-radius: 999px;
    backdrop-filter: blur(var(--blur));
  }
  .tabs button {
    padding: 5px 16px;
    border-radius: 999px;
    color: var(--fg-dim);
    font-size: 13px;
    transition: background 0.18s, color 0.18s;
  }
  .tabs button:hover {
    color: var(--fg);
  }
  .tabs button.on {
    background: var(--glass-strong);
    color: var(--fg);
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 9px;
    max-width: 190px;
    padding: 4px 12px 4px 4px;
    border-radius: 999px;
    font-size: 13px;
    background: var(--glass);
    border: 1px solid var(--hairline);
    backdrop-filter: blur(var(--blur));
  }
  .who:hover {
    background: var(--glass-hover);
  }
  .who img,
  .who .av {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    object-fit: cover;
    flex: none;
    background: linear-gradient(135deg, #5c8dff, #b06ad9);
  }

  .wctl {
    display: flex;
    margin-left: 8px;
  }
  .wctl button {
    width: 42px;
    height: 46px;
    border-radius: 0;
    color: var(--fg-dim);
    font-size: 11px;
  }
  .wctl button:hover {
    background: rgba(255, 255, 255, 0.08);
    color: var(--fg);
  }
  .wctl button.x:hover {
    background: #c42b1c;
    color: #fff;
  }

  .banner {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin: 0 18px;
    padding: 8px 16px;
    border-radius: var(--r-sm);
    background: rgba(180, 50, 60, 0.22);
    border: 1px solid rgba(220, 90, 100, 0.35);
    backdrop-filter: blur(var(--blur));
  }

  main {
    overflow-y: auto;
    min-height: 0;
    padding: 0 30px 8px;
  }
</style>
