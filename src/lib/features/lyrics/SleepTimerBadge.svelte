<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import * as api from "../../api";
  import { store } from "../../store.svelte";
  import SelectMenu, { type SelectOption } from "../../ui/SelectMenu.svelte";
  import type { SleepTimerStatus } from "../../types";

  let sleepTimer = $state<SleepTimerStatus | null>(null);
  let sleepClockMs = $state(Date.now());

  const sleepRemainingSeconds = $derived(
    sleepTimer?.mode === "duration" && sleepTimer.endsAtUnixMs !== null
      ? Math.max(0, Math.ceil((sleepTimer.endsAtUnixMs - sleepClockMs) / 1000))
      : (sleepTimer?.remainingSeconds ?? null),
  );

  const sleepTimerLabel = $derived(
    sleepTimer?.active
      ? sleepTimer.mode === "endOfTrack"
        ? "Sleep · end of track"
        : `Sleep · ${Math.floor((sleepRemainingSeconds ?? 0) / 60)}:${String((sleepRemainingSeconds ?? 0) % 60).padStart(2, "0")}`
      : "Sleep timer"
  );
  const sleepTimerOptions = $derived<SelectOption[]>([
    { value: "15", label: "15 minutes" },
    { value: "30", label: "30 minutes" },
    { value: "60", label: "1 hour" },
    { value: "end", label: "End of track" },
    ...(sleepTimer?.active
      ? [{ value: "cancel", label: "Cancel timer", tone: "warning" as const }]
      : []),
  ]);

  async function chooseSleep(value: string | null) {
    if (!value) return;
    await store.run(async () => {
      sleepTimer =
        value === "end"
          ? await api.sleepAtEndOfTrack()
          : value === "cancel"
            ? await api.cancelSleepTimer()
            : await api.startSleepTimer(Number(value) * 60);
    });
  }

  onMount(() => {
    let disposed = false;
    let stop: UnlistenFn | undefined;
    api.getSleepTimer().then((value) => {
      if (!disposed) sleepTimer = value;
    });
    listen<SleepTimerStatus>(api.EVENT_SLEEP_TIMER, (event) => {
      sleepTimer = event.payload;
    }).then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    return () => {
      disposed = true;
      stop?.();
    };
  });

  onMount(() => {
    const timer = window.setInterval(() => (sleepClockMs = Date.now()), 1000);
    return () => clearInterval(timer);
  });
</script>

<SelectMenu
  value={null}
  options={sleepTimerOptions}
  triggerLabel={sleepTimerLabel}
  label="Sleep timer"
  compact
  onChange={chooseSleep}
>
  {#snippet icon()}
    <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="13" r="8" /><path d="M12 9v4l3 2" /><path d="M9 2h6" />
    </svg>
  {/snippet}
</SelectMenu>
