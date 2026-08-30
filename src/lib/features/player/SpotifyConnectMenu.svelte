<script lang="ts">
  import { tick } from "svelte";
  import * as api from "../../api";
  import { store } from "../../store.svelte";
  import type { Device } from "../../types";

  let open = $state(false);
  let devices = $state<Device[]>([]);
  let loading = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let panel = $state<HTMLElement | null>(null);
  let panelStyle = $state("");

  /* The trigger highlights when playback is happening *somewhere else*, not
     here. Accenting it while this app is the active device made the icon lit
     during ordinary local listening, which reads as "you are casting" — the
     opposite of the truth. There is nothing to point at when nothing is
     playing at all, hence the track check. */
  const remotePlayback = $derived(
    !store.playback.isActiveDevice && store.playback.track !== null,
  );

  $effect(() => {
    const live = store.playback.availableDevices;
    if (live.length) devices = live;
  });

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  async function refresh() {
    loading = true;
    try {
      devices = await api.listDevices();
    } catch (error) {
      store.handleError(error);
    } finally {
      loading = false;
    }
  }

  async function show() {
    open = true;
    void refresh();
    await tick();
    const rect = trigger?.getBoundingClientRect();
    if (rect) {
      const right = Math.max(8, window.innerWidth - rect.right);
      const bottom = Math.max(8, window.innerHeight - rect.top + 8);
      const maxHeight = Math.max(180, rect.top - 24);
      panelStyle = `right:${right}px;bottom:${bottom}px;max-height:${maxHeight}px`;
    }
    panel?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }

  function close(restoreFocus = true) {
    open = false;
    if (restoreFocus) requestAnimationFrame(() => trigger?.focus());
  }

  async function pick(device: Device) {
    if (!device.id) return;
    await store.run(() => api.transferPlayback(device.id!, true));
    await refresh();
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const items = Array.from(
      panel?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
    );
    if (!items.length) return;
    const current = Math.max(0, items.indexOf(document.activeElement as HTMLButtonElement));
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : (current + (event.key === "ArrowDown" ? 1 : -1) + items.length) %
            items.length;
    items[next].focus();
  }
</script>

<div class="picker">
  <button
    bind:this={trigger}
    class="trigger"
    class:active={remotePlayback}
    onclick={() => (open ? close(false) : void show())}
    aria-haspopup="menu"
    aria-expanded={open}
    title={remotePlayback
      ? `Playing on ${store.playback.activeDevice?.name ?? "another device"}`
      : `Connect: ${store.playback.connectionStatus}`}
    aria-label={remotePlayback
      ? `Spotify Connect — playing on ${store.playback.activeDevice?.name ?? "another device"}`
      : `Spotify Connect playback device — ${store.playback.connectionStatus}`}
  >
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <rect x="2" y="4" width="14" height="10" rx="2" />
      <rect x="17" y="9" width="5" height="11" rx="1.5" />
    </svg>
  </button>

  {#if open}
    <button
      use:portal
      class="backdrop"
      onclick={() => close()}
      aria-label="Close Spotify Connect menu"
      tabindex="-1"
    ></button>
    <div
      use:portal
      bind:this={panel}
      class="panel menu-surface"
      style={panelStyle}
      role="menu"
      aria-label="Spotify Connect devices"
      tabindex="-1"
      onkeydown={keydown}
    >
      <header>
        <span>Spotify Connect</span>
        <button class="refresh" onclick={refresh} title="Refresh devices" aria-label="Refresh devices">↻</button>
      </header>

      <div class="device-list">
        {#if loading && !devices.length}
          <p class="muted pad">Scanning…</p>
        {:else}
          {#each devices as device (device.id ?? device.name)}
            <button
              class="device menu-item"
              role="menuitemradio"
              aria-checked={device.isActive}
              onclick={() => void pick(device)}
              disabled={!device.id}
            >
              <span class="dot" class:on={device.isActive}></span>
              <span class="device-meta">
                <span class="truncate">{device.name}</span>
                <span class="muted small">{device.type}</span>
              </span>
            </button>
          {:else}
            <p class="muted pad">No Spotify Connect devices found.</p>
          {/each}
        {/if}
      </div>

      <button
        class="play-here menu-item"
        role="menuitem"
        onclick={() => store.run(api.activateThisDevice)}
      >
        Play on this device
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
    border-radius: var(--control-radius);
    color: var(--fg-dim);
  }
  .trigger:hover,
  .trigger:focus-visible {
    background: var(--glass-hover);
    color: var(--fg);
  }
  .trigger.active {
    color: var(--accent);
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay);
    background: transparent;
  }
  .panel {
    position: fixed;
    z-index: var(--z-menu);
    width: min(280px, calc(100vw - 16px));
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    flex: none;
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 6px 8px 9px;
    font-weight: 600;
  }
  .refresh {
    width: 28px;
    height: 28px;
    border-radius: var(--control-radius);
    color: var(--fg-dim);
  }
  .refresh:hover {
    color: var(--fg);
    background: var(--glass-hover);
  }
  .device-list {
    min-height: 0;
    overflow-y: auto;
  }
  .device {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .device-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: #4a4038;
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
  .play-here {
    flex: none;
    margin-top: 6px;
    border-top: 1px solid var(--hairline);
    color: var(--fg-dim);
  }
</style>
