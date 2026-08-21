<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  // The bundle icon itself, not a copy of it — one source of truth, so
  // regenerating the icon set updates the titlebar mark too. `src-tauri/` is
  // inside the Vite root so this resolves and gets emitted as a hashed asset
  // (same-origin, which the CSP's `img-src 'self'` covers). It is excluded from
  // the dev watcher in vite.config.ts, so changing the icon needs a restart
  // rather than showing up on HMR.
  import iconUrl from "../src-tauri/icons/64x64.png";
  import { store } from "./lib/store.svelte";
  import PlayerBar from "./lib/features/player/PlayerBar.svelte";
  import AlbumView from "./lib/views/AlbumView.svelte";
  import Home from "./lib/views/Home.svelte";
  import Jams from "./lib/views/Jams.svelte";
  import Library from "./lib/views/Library.svelte";
  import Login from "./lib/views/Login.svelte";
  import NowPlaying from "./lib/features/lyrics/NowPlaying.svelte";
  import Search from "./lib/views/Search.svelte";
  import Settings from "./lib/features/settings/Settings.svelte";
  import Profile from "./lib/views/Profile.svelte";
  import Setup from "./lib/views/Setup.svelte";
  import Releases from "./lib/views/Releases.svelte";
  import ForYou from "./lib/views/ForYou.svelte";
  import type { AlbumSummary } from "./lib/types";

  type Tab = "home" | "search" | "releases" | "library" | "forYou" | "jams" | "settings";

  let tab = $state<Tab>("home");
  let nowPlayingOpen = $state(false);
  let profileOpen = $state(false);
  let playerFullscreen = $state(false);
  let fullscreenLyricsRequested = $state(false);
  let openAlbum = $state<AlbumSummary | null>(null);
  let main: HTMLElement | null = $state(null);

  const appWindow = getCurrentWindow();

  function go(next: Tab) {
    tab = next;
    nowPlayingOpen = false;
    profileOpen = false;
    openAlbum = null;
    fullscreenLyricsRequested = false;
    // Each tab keeps its own component state, but they share one scroller.
    if (main) main.scrollTop = 0;
  }

  async function reconfigure() {
    await store.run(async () => {
      await store.logout();
      store.setupNeeded = true;
    });
  }

  function openArtworkDestination() {
    const track = store.playback.track;
    if (!track) return;
    if (track.albumId && track.albumUri) {
      openAlbum = {
        id: track.albumId,
        uri: track.albumUri,
        name: track.album,
        artists: track.artists,
        imageUrl: track.coverUrl,
      };
      nowPlayingOpen = false;
      profileOpen = false;
      fullscreenLyricsRequested = false;
      return;
    }
    nowPlayingOpen = true;
    profileOpen = false;
    fullscreenLyricsRequested = false;
  }

  function openNowPlaying(fullscreenLyrics = false) {
    openAlbum = null;
    profileOpen = false;
    nowPlayingOpen = true;
    fullscreenLyricsRequested = fullscreenLyrics;
  }

  onMount(() => store.init());
  onDestroy(() => store.destroy());
</script>

<div class="root" class:reduce-motion={store.settings.reduceMotion}>
  <!-- Colour wash behind every panel. Without something to refract, glass
       reads as flat grey. -->
  <div class="ambient"></div>
  <div class="veil"></div>

  {#if store.booting}
    <div class="boot"><p class="muted">Starting…</p></div>
  {:else if store.setupNeeded || !store.auth.loggedIn}
    <!-- The title bar is gone with decorations, so Setup/Login need their own
         drag strip or the window becomes unmovable before sign-in. -->
    <div class="preauth">
      <div class="predrag" data-tauri-drag-region>
        <div class="wctl">
          <button onclick={() => appWindow.minimize()} title="Minimize">&#9472;</button>
          <button onclick={() => appWindow.toggleMaximize()} title="Maximize">&#9723;</button>
          <button class="x" onclick={() => appWindow.close()} title="Close">&#10005;</button>
        </div>
      </div>
      {#if store.setupNeeded}
        <Setup onDone={() => store.finishSetup()} onPair={() => (store.setupNeeded = false)} />
      {:else}
        <Login />
      {/if}
    </div>
  {:else}
    <div class="shell" class:fullscreen-player={playerFullscreen}>
      <header class="titlebar" data-tauri-drag-region>
        <div class="mark" data-tauri-drag-region>
          <img class="logo" src={iconUrl} alt="" width="20" height="20" draggable="false" />
          Rustify
        </div>

        <nav class="tabs">
          <button class:on={tab === "home"} aria-current={tab === "home" ? "page" : undefined} onclick={() => go("home")}>Home</button>
          <button class:on={tab === "search"} aria-current={tab === "search" ? "page" : undefined} onclick={() => go("search")}>
            Search
          </button>
          <button class:on={tab === "releases"} aria-current={tab === "releases" ? "page" : undefined} onclick={() => go("releases")}>Releases</button>
          <button class:on={tab === "library"} aria-current={tab === "library" ? "page" : undefined} onclick={() => go("library")}>
            Library
          </button>
          <button class:on={tab === "jams"} aria-current={tab === "jams" ? "page" : undefined} onclick={() => go("jams")}>
            Jams
          </button>
          <button class:on={tab === "settings"} aria-current={tab === "settings" ? "page" : undefined} onclick={() => go("settings")}>
            Settings
          </button>
        </nav>

        <div class="spacer" data-tauri-drag-region></div>

        <button
          class="who"
          onclick={() => {
            profileOpen = true;
            nowPlayingOpen = false;
            openAlbum = null;
            fullscreenLyricsRequested = false;
          }}
          title="Open profile"
        >
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

      <main bind:this={main} class:player-view={nowPlayingOpen}>
        {#if openAlbum}
          <AlbumView album={openAlbum} onBack={() => (openAlbum = null)} />
        {:else if profileOpen}
          <Profile onBack={() => (profileOpen = false)} />
        {:else if nowPlayingOpen}
          <NowPlaying
            onClose={() => {
              nowPlayingOpen = false;
              fullscreenLyricsRequested = false;
            }}
            onNavigateArtwork={openArtworkDestination}
            startFullscreen={fullscreenLyricsRequested}
            onFullscreenChange={(value) => {
              playerFullscreen = value;
              if (!value) fullscreenLyricsRequested = false;
            }}
          />
        {:else if tab === "home"}
          <Home onBrowseLibrary={() => go("library")} onOpenForYou={() => go("forYou")} onOpenReleases={() => go("releases")} />
        {:else if tab === "search"}
          <Search />
        {:else if tab === "releases"}
          <Releases />
        {:else if tab === "forYou"}
          <ForYou />
        {:else if tab === "library"}
          <Library />
        {:else if tab === "jams"}
          <Jams />
        {:else}
          <Settings onReconfigure={reconfigure} />
        {/if}
      </main>

      {#if !playerFullscreen}
        <PlayerBar
          onOpenNowPlaying={() =>
            nowPlayingOpen ? (nowPlayingOpen = false) : openNowPlaying(false)}
          onOpenArtwork={openArtworkDestination}
          onOpenLyrics={() => openNowPlaying(true)}
        />
      {/if}
    </div>
  {/if}
</div>

<style>
  .root {
    position: relative;
    height: 100%;
    overflow: hidden;
    isolation: isolate;
  }

  /* Three drifting blobs, deliberately slow: at 26s the movement is felt
     rather than watched.
     No opaque base colour and `opacity` well under 1: this layer sits on top
     of the Acrylic backdrop, so anything solid here erases it. The blobs are
     a tint over whatever is behind the window now, not a background of their
     own.

     Acrylic (unlike Mica) shows real content behind the window, which can be
     arbitrarily saturated — a wallpaper, another app. A flat alpha-blended
     patch of solid colour over that reads as a muddy, clashing smear rather
     than a tint, because plain "over" compositing is a linear mix, not
     anything perceptual. Lower opacity and a soft blur (no hard gradient
     edges left to collide with what's behind) fix that; saturate(0.85) keeps
     the blobs from being the most intense colour on screen even over a dim
     backdrop. */
  .ambient {
    position: absolute;
    inset: -30%;
    z-index: -2;
    opacity: 0.22;
    background:
      radial-gradient(42% 46% at 20% 16%, #8c6239 0%, transparent 70%),
      radial-gradient(38% 42% at 84% 24%, #6b4226 0%, transparent 70%),
      radial-gradient(50% 48% at 60% 90%, #b08046 0%, transparent 68%);
    /* Sepia: tobacco, deep umber, caramel. Three warm tones rather than one
       flat brown — with a single hue the blobs stop reading as separate blobs
       and the drift below becomes invisible. */
    filter: saturate(0.85) blur(70px);
    animation: drift 26s ease-in-out infinite alternate;
  }
  .reduce-motion .ambient { animation: none; }
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
    /* Balancing act: dark enough that white text stays legible over the blobs
       and over whatever is showing through Acrylic, light enough that both
       survive. Every point added here is a point of Acrylic removed, so this
       is deliberately barely-there — lighter still than the .04/.2 it used
       under Mica, since Acrylic's own blur already does legibility work Mica
       never did. */
    background: linear-gradient(180deg, rgba(14, 9, 4, 0.02), rgba(14, 9, 4, 0.1));
  }
  .veil::after {
    content: "";
    position: absolute;
    inset: 0;
    opacity: 0.1;
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.9' numOctaves='3'/%3E%3C/filter%3E%3Crect width='120' height='120' filter='url(%23n)'/%3E%3C/svg%3E");
  }

  .boot {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
  }
  .predrag {
    display: flex;
    justify-content: flex-end;
    height: 46px;
  }
  .preauth {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .preauth .predrag { flex: none; }

  /* Pinned to the root's edges rather than `height: 100%`. A percentage height
     silently collapses to content height if any ancestor's height is not
     definite, which left the player bar floating mid-window with dead space
     below it whenever the view was short. `inset: 0` cannot fail that way. */
  /* Flex, not `grid-template-rows: auto auto 1fr auto` — the error banner is
     conditional, so with no banner there were only three children and the
     `1fr` landed on the *player bar* instead of `main`. That collapsed the
     content area to its own height and left the player floating mid-window
     with dead space beneath it. Flex assigns the stretch by rule, not by
     child position, so a missing banner cannot shift it. */
  .shell {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
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
  /* 20px, not the 9px the dot used: the icon is a llama inside a disc, and
     below roughly 18px it stops resolving as anything and just reads as a
     green blob — worse than the dot it replaced. The titlebar is 46px, so
     20px sits comfortably without pushing the row taller.
     `pointer-events: none` keeps it from swallowing drags: the mark is a
     `data-tauri-drag-region`, and a child image would otherwise be a dead spot
     in the strip you grab to move the window. */
  .mark .logo {
    width: 20px;
    height: 20px;
    flex: none;
    pointer-events: none;
    user-select: none;
    filter: drop-shadow(0 1px 3px rgba(18, 11, 4, 0.55));
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
    color: rgba(245, 240, 230, 0.72);
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
    background: linear-gradient(135deg, #b08046, #6b4226);
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
    background: rgba(255, 241, 224, 0.08);
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
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    padding: 0 30px 8px;
  }
  .shell.fullscreen-player > .titlebar,
  .shell.fullscreen-player > .banner { display: none; }
  main.player-view { padding: 0; overflow: hidden; }
</style>
