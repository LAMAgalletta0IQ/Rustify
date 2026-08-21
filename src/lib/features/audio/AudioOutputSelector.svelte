<script lang="ts">
  import { onMount } from "svelte";
  import SelectMenu, { type SelectOption } from "../../ui/SelectMenu.svelte";
  import { audioDevices } from "./audio-devices.svelte";

  let {
    value = $bindable<string | null>(),
    compact = false,
    disabled = false,
    onChange,
  }: {
    value?: string | null;
    compact?: boolean;
    disabled?: boolean;
    onChange: (value: string | null) => void | Promise<void>;
  } = $props();

  const options = $derived.by<SelectOption[]>(() => {
    const defaultDevice = audioDevices.devices.find((device) => device.isDefault);
    return [
      {
        value: null,
        label: "System default",
        description: defaultDevice
          ? `Currently ${defaultDevice.name}`
          : "Follow the Windows default output",
      },
      ...audioDevices.devices.map((device) => ({
        value: device.id,
        label: device.name,
        description: !device.isAvailable
          ? "Disconnected · using system default"
          : device.isActive
            ? "Active output"
            : device.isDefault
              ? "Windows default"
              : "Available",
        disabled: !device.isAvailable,
        tone: !device.isAvailable
          ? ("warning" as const)
          : device.isActive
            ? ("active" as const)
            : ("default" as const),
      })),
    ];
  });

  onMount(() => audioDevices.subscribe());

  async function select(next: string | null) {
    await onChange(next);
    await audioDevices.refresh();
  }
</script>

<div class="output-selector" class:compact>
  <SelectMenu
    bind:value
    {options}
    {compact}
    disabled={disabled || audioDevices.loading && options.length === 1}
    label="Audio output device"
    onChange={(next) => void select(next)}
  />
  {#if !compact && audioDevices.error}
    <p class="device-error" role="status">{audioDevices.error}</p>
  {/if}
</div>

<style>
  .output-selector { display: flex; min-width: 0; flex-direction: column; align-items: flex-end; gap: 6px; }
  .output-selector.compact { display: block; }
  .device-error { max-width: 330px; margin: 0; color: var(--warning); font-size: 10px; text-align: right; }
</style>
