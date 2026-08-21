<script lang="ts">
  import * as api from "../api";
  import { store } from "../store.svelte";
  import type { Device } from "../types";

  let open = $state(false);
  let devices = $state<Device[]>([]);
  let loading = $state(false);

  $effect(() => {
    const live = store.playback.availableDevices;
    if (live.length) devices = live;
  });

  async function refresh() {
    loading = true;
    try {
      devices = await api.listDevices();
    } catch (e) {
      store.handleError(e);
    } finally {
      loading = false;
    }
  }

  function toggle() {
    open = !open;
    if (open) refresh();
  }

  async function pick(d: Device) {
    if (!d.id) return;
    await store.run(() => api.transferPlayback(d.id!, true));
    await refresh();
  }
</script>

<div class="picker">
  <button
    class="trigger"
    onclick={toggle}
    title={`Connect: ${store.playback.connectionStatus}`}
    class:active={store.playback.isActiveDevice}
  >
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <rect x="2" y="4" width="14" height="10" rx="2" />
      <rect x="17" y="9" width="5" height="11" rx="1.5" />
    </svg>
  </button>

  {#if open}
    <!-- Click-away backdrop; keeps the popover logic to a single element. -->
    <button class="backdrop" onclick={() => (open = false)} aria-label="Close"
    ></button>
    <div class="panel">
      <header>
        <span>Connect to a device</span>
        <button class="refresh" onclick={refresh} title="Refresh">⟳</button>
      </header>

      {#if loading}
        <p class="muted pad">Scanning…</p>
      {:else}
        {#each devices as d (d.id ?? d.name)}
          <button class="dev" onclick={() => pick(d)} disabled={!d.id}>
            <span class="dot" class:on={d.isActive}></span>
            <span class="dmeta">
              <span class="truncate">{d.name}</span>
              <span class="muted small">{d.type}</span>
            </span>
          </button>
        {:else}
          <p class="muted pad">No devices found.</p>
        {/each}
      {/if}

      <button class="here" onclick={() => store.run(api.activateThisDevice)}>
        Play here
      </button>
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  .trigger {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 8px;
    color: var(--fg-dim);
  }
  .trigger:hover {
    background: var(--glass-hover);
    color: var(--fg);
  }
  .trigger.active {
    color: var(--accent);
  }
  .backdrop {
    position: fixed;
    inset: 0;
    background: transparent;
    z-index: 10;
  }
  .panel {
    position: absolute;
    bottom: calc(100% + 8px);
    right: 0;
    z-index: 11;
    width: 260px;
    padding: 8px;
    /* A popover needs its own darkening: pure --glass over the player bar's
       glass would stack two translucent layers and read as unreadable haze.
       Kept opaque-ish rather than promoted to `.raised` for the same reason —
       this is the one surface that lands on top of another glass panel, and
       Acrylic cannot be relied on to darken what shows through it. */
    background: rgba(30, 22, 14, 0.86);
    border: 1px solid var(--hairline);
    border-radius: var(--r-md);
    backdrop-filter: blur(var(--blur));
    box-shadow: 0 12px 32px rgba(18, 11, 4, 0.5), var(--edge);
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 8px 8px;
    font-weight: 600;
  }
  .refresh {
    color: var(--fg-dim);
  }
  .dev {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px;
    text-align: left;
  }
  .dev:hover:not(:disabled) {
    background: var(--glass-hover);
    border-radius: var(--r-sm);
  }
  .dmeta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #4a4038;
    flex: none;
  }
  .dot.on {
    background: var(--accent);
  }
  .small {
    font-size: 11px;
  }
  .pad {
    padding: 8px;
  }
  .here {
    width: 100%;
    margin-top: 6px;
    padding: 8px;
    border-top: 1px solid var(--hairline);
    color: var(--fg-dim);
  }
  .here:hover {
    color: var(--fg);
  }
</style>
