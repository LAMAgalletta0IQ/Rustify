<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import * as api from "./lib/api";
  import { store } from "./lib/store.svelte";
  import PlayerBar from "./lib/components/PlayerBar.svelte";
  import Home from "./lib/views/Home.svelte";
  import Login from "./lib/views/Login.svelte";
  import NowPlaying from "./lib/views/NowPlaying.svelte";
  import Search from "./lib/views/Search.svelte";

  type Tab = "home" | "search";

  let tab = $state<Tab>("home");
  let nowPlayingOpen = $state(false);

  onMount(() => store.init());
  onDestroy(() => store.destroy());
</script>

{#if store.booting}
  <div class="boot"><p class="muted">Starting…</p></div>
{:else if !store.auth.loggedIn}
  <Login />
{:else}
  <div class="shell">
    <header class="topbar">
      <nav>
        <button class:on={tab === "home"} onclick={() => (tab = "home")}>
          Home
        </button>
        <button class:on={tab === "search"} onclick={() => (tab = "search")}>
          Search
        </button>
      </nav>

      <div class="user">
        {#if store.auth.avatarUrl}
          <img src={store.auth.avatarUrl} alt="" width="26" height="26" />
        {/if}
        <span class="muted truncate">{store.auth.displayName ?? ""}</span>
        <button class="logout muted" onclick={() => store.run(api.logout)}>
          Log out
        </button>
      </div>
    </header>

    {#if store.error}
      <div class="banner">
        <span class="truncate">{store.error}</span>
        <button onclick={() => (store.error = null)}>✕</button>
      </div>
    {/if}

    <main>
      {#if nowPlayingOpen}
        <NowPlaying onClose={() => (nowPlayingOpen = false)} />
      {:else if tab === "home"}
        <Home />
      {:else}
        <Search />
      {/if}
    </main>

    <PlayerBar onOpenNowPlaying={() => (nowPlayingOpen = !nowPlayingOpen)} />
  </div>
{/if}

<style>
  .boot {
    display: grid;
    place-items: center;
    height: 100%;
  }
  .shell {
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    height: 100%;
  }
  .topbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border);
  }
  nav {
    display: flex;
    gap: 4px;
  }
  nav button {
    padding: 6px 14px;
    color: var(--fg-dim);
  }
  nav button.on {
    color: var(--fg);
    background: var(--bg-elev-2);
  }
  .user {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .user img {
    border-radius: 50%;
  }
  .logout {
    font-size: 12px;
    padding: 4px 8px;
  }
  .logout:hover {
    color: var(--fg);
  }
  .banner {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 20px;
    background: #2a1618;
    border-bottom: 1px solid #57282c;
  }
  main {
    overflow-y: auto;
    min-height: 0;
  }
</style>
