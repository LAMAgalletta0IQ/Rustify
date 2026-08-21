<script lang="ts" module>
  export interface SelectOption {
    value: string | null;
    label: string;
    description?: string;
    disabled?: boolean;
    tone?: "default" | "warning" | "active";
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";

  let {
    value = $bindable<string | null>(),
    options,
    label,
    disabled = false,
    compact = false,
    // For transient action menus (sleep timer, "add to playlist"…) where no
    // option represents a persisted current value — the trigger shows this
    // instead of falling back to the first option's label. `value` in that
    // mode is just "which item last fired", not a selection to redisplay.
    triggerLabel,
    onChange = (_value: string | null) => {},
  }: {
    value?: string | null;
    options: SelectOption[];
    label: string;
    disabled?: boolean;
    compact?: boolean;
    triggerLabel?: string;
    onChange?: (value: string | null) => void;
  } = $props();

  let open = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let panel = $state<HTMLElement | null>(null);
  let activeIndex = $state(0);
  let panelStyle = $state("");

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  const selected = $derived(
    options.find((option) => option.value === value) ?? options[0],
  );
  const displayLabel = $derived(triggerLabel ?? selected?.label ?? "Select");

  function enabledIndex(start: number, direction: 1 | -1) {
    if (!options.length) return -1;
    for (let offset = 0; offset < options.length; offset += 1) {
      const index = (start + direction * offset + options.length) % options.length;
      if (!options[index]?.disabled) return index;
    }
    return -1;
  }

  function focusActive() {
    panel
      ?.querySelector<HTMLButtonElement>(`[data-option-index="${activeIndex}"]`)
      ?.focus({ preventScroll: true });
  }

  function positionPanel() {
    if (!trigger || !panel) return;
    const rect = trigger.getBoundingClientRect();
    const margin = 10;
    const width = Math.max(220, rect.width);
    const availableBelow = window.innerHeight - rect.bottom - margin;
    const availableAbove = rect.top - margin;
    const maxHeight = Math.max(150, Math.min(360, Math.max(availableBelow, availableAbove)));
    const useAbove = availableBelow < Math.min(panel.scrollHeight, 220) && availableAbove > availableBelow;
    const top = useAbove
      ? Math.max(margin, rect.top - Math.min(panel.scrollHeight, maxHeight) - 7)
      : Math.min(window.innerHeight - margin, rect.bottom + 7);
    const left = Math.min(
      window.innerWidth - width - margin,
      Math.max(margin, rect.right - width),
    );
    panelStyle = `left:${left}px;top:${top}px;width:${width}px;max-height:${maxHeight}px`;
  }

  async function show(direction: 1 | -1 = 1) {
    if (disabled || open) return;
    const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value));
    activeIndex = enabledIndex(selectedIndex, direction);
    open = true;
    await tick();
    positionPanel();
    focusActive();
  }

  function hide(restoreFocus = true) {
    if (!open) return;
    open = false;
    if (restoreFocus) queueMicrotask(() => trigger?.focus({ preventScroll: true }));
  }

  function choose(index: number) {
    const option = options[index];
    if (!option || option.disabled) return;
    value = option.value;
    onChange(option.value);
    hide();
  }

  function onTriggerKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      void show(event.key === "ArrowDown" ? 1 : -1);
    }
  }

  function onPanelKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      hide();
      return;
    }
    if (event.key === "Tab") {
      hide(false);
      return;
    }
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      const direction = event.key === "ArrowUp" || event.key === "End" ? -1 : 1;
      const start = event.key === "Home"
        ? 0
        : event.key === "End"
          ? options.length - 1
          : activeIndex + direction;
      activeIndex = enabledIndex(start, direction);
      focusActive();
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(activeIndex);
    }
  }

  onMount(() => {
    const reposition = () => open && positionPanel();
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", reposition, true);
    return () => {
      window.removeEventListener("resize", reposition);
      window.removeEventListener("scroll", reposition, true);
    };
  });
</script>

<button
  bind:this={trigger}
  class="select-trigger"
  class:compact
  type="button"
  aria-label={label}
  aria-haspopup="listbox"
  aria-expanded={open}
  {disabled}
  onclick={() => (open ? hide() : void show())}
  onkeydown={onTriggerKeydown}
>
  {#if compact}
    <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M5 9v6h4l5 4V5L9 9z" /><path d="M18 9.5a4 4 0 0 1 0 5" />
    </svg>
    <span class="compact-label truncate">{displayLabel}</span>
  {:else}
    <span class="truncate">{displayLabel}</span>
  {/if}
  <svg class="chevron" aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2">
    <path d="m7 10 5 5 5-5" />
  </svg>
</button>

{#if open}
  <button use:portal class="menu-backdrop" type="button" tabindex="-1" aria-label={`Close ${label}`} onclick={() => hide()}></button>
  <div
    use:portal
    bind:this={panel}
    class="menu-surface select-panel"
    style={panelStyle}
    role="listbox"
    aria-label={label}
    tabindex="-1"
    onkeydown={onPanelKeydown}
  >
    {#each options as option, index (`${option.value}-${index}`)}
      <button
        class="menu-item option"
        class:selected={option.value === value}
        class:warning={option.tone === "warning"}
        role="option"
        aria-selected={option.value === value}
        data-option-index={index}
        tabindex={index === activeIndex ? 0 : -1}
        disabled={option.disabled}
        onfocus={() => (activeIndex = index)}
        onclick={() => choose(index)}
      >
        <span class="option-copy">
          <strong>{option.label}</strong>
          {#if option.description}<small>{option.description}</small>{/if}
        </span>
        {#if option.value === value}<span class="check" aria-hidden="true">✓</span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .select-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: min(330px, 100%);
    min-height: var(--control-height);
    padding: 8px 11px 8px 13px;
    color: var(--fg);
    background: var(--control-bg);
    border: 1px solid var(--control-border);
    border-radius: var(--control-radius);
    text-align: left;
  }
  .select-trigger:hover:not(:disabled),
  .select-trigger[aria-expanded="true"] {
    background: var(--control-hover);
    border-color: var(--control-border-hover);
  }
  .select-trigger:disabled { opacity: var(--disabled-opacity); }
  .select-trigger.compact {
    /* min-width:0 is what actually lets this shrink below its content's
       natural size inside a flex row (PlayerBar's .right, here or anywhere
       else this is used compactly) — width/max-width alone cap growth, they
       don't override the flex default of "never shrink below content",
       which is what let the label run into its neighbors instead of eliding. */
    min-width: 0;
    width: auto;
    max-width: 190px;
    flex-shrink: 1;
    min-height: 30px;
    padding: 6px 8px;
    color: var(--fg-dim);
  }
  .select-trigger.compact:hover,
  .select-trigger.compact[aria-expanded="true"] { color: var(--fg); }
  .compact-label { min-width: 0; max-width: 130px; font-size: 11px; }
  .select-trigger svg { flex: none; }
  .select-trigger .chevron { transition: transform var(--motion-fast); }
  .select-trigger[aria-expanded="true"] .chevron { transform: rotate(180deg); }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay-backdrop);
    padding: 0;
    border-radius: 0;
    background: transparent;
  }
  .select-panel {
    position: fixed;
    z-index: var(--z-overlay);
    overflow: auto;
    overscroll-behavior: contain;
  }
  .option { justify-content: space-between; }
  .option-copy { display: flex; min-width: 0; flex-direction: column; gap: 2px; }
  .option-copy strong { font-size: 12px; font-weight: 600; }
  .option-copy small { color: var(--fg-dim); font-size: 10px; }
  .option.selected { color: var(--accent); background: var(--menu-selected); }
  .option.warning small { color: var(--warning); }
  .check { flex: none; font-weight: 700; }
  @media (prefers-reduced-motion: reduce) {
    .select-trigger .chevron { transition: none; }
  }
</style>
