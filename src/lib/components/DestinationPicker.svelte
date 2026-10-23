<script lang="ts">
  import type { PresetSummary } from "$lib/types";

  interface Props {
    presets: PresetSummary[];
    presetId: string;
    disabled?: boolean;
    onselect: (id: string) => void;
  }
  let { presets, presetId, disabled = false, onselect }: Props = $props();

  /** Presets grouped into destinations, in table order (Rust keeps groups contiguous). */
  const groups = $derived.by(() => {
    const list: { name: string; presets: PresetSummary[] }[] = [];
    for (const p of presets) {
      const last = list.at(-1);
      if (last?.name === p.group) last.presets.push(p);
      else list.push({ name: p.group, presets: [p] });
    }
    return list;
  });

  /** The tier last picked in each group, so switching away from Discord and back keeps "Nitro". */
  let lastTier = $state<Record<string, string>>({});
  $effect(() => {
    const current = presets.find((p) => p.id === presetId);
    if (current) lastTier[current.group] = current.id;
  });

  const current = $derived(presets.find((p) => p.id === presetId));
  const chosenIn = (group: { name: string; presets: PresetSummary[] }) =>
    group.presets.find((p) => p.id === lastTier[group.name]) ?? group.presets[0];

  let list = $state<HTMLElement>();

  /** Arrow keys move the selection, like native radio buttons. */
  function onGroupKey(event: KeyboardEvent, index: number) {
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const next = groups[(index + step + groups.length) % groups.length];
    onselect(chosenIn(next).id);
    list?.querySelectorAll<HTMLButtonElement>("[data-group]")[groups.indexOf(next)]?.focus();
  }

  function onTierKey(event: KeyboardEvent, group: { presets: PresetSummary[] }, index: number) {
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const nextIndex = (index + step + group.presets.length) % group.presets.length;
    onselect(group.presets[nextIndex].id);
    const siblings = (event.currentTarget as HTMLElement).parentElement?.children;
    (siblings?.[nextIndex] as HTMLElement | undefined)?.focus();
  }
</script>

<section class="picker">
  <h2 id="send-to">Send to</h2>
  <div class="list" role="radiogroup" aria-labelledby="send-to" bind:this={list}>
    {#each groups as group, i (group.name)}
      {@const selected = current?.group === group.name}
      {@const chosen = chosenIn(group)}
      <div class="row" class:selected>
        <button
          class="destination"
          role="radio"
          aria-checked={selected}
          tabindex={selected ? 0 : -1}
          data-group
          {disabled}
          onclick={() => onselect(chosen.id)}
          onkeydown={(e) => onGroupKey(e, i)}
        >
          <span class="dot" aria-hidden="true"></span>
          <span class="name">{group.name}</span>
          <span class="hint">{chosen.hint}</span>
        </button>

        {#if group.presets.length > 1}
          <div class="tiers" role="radiogroup" aria-label="{group.name} plan">
            {#each group.presets as preset, t (preset.id)}
              <button
                role="radio"
                aria-checked={preset.id === chosen.id}
                class:active={selected && preset.id === chosen.id}
                class:remembered={!selected && preset.id === chosen.id}
                tabindex={preset.id === chosen.id ? 0 : -1}
                {disabled}
                onclick={() => onselect(preset.id)}
                onkeydown={(e) => onTierKey(e, group, t)}
              >
                {preset.tier}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
</section>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h2 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 400;
    color: var(--muted);
  }

  .row {
    border-bottom: 1px solid var(--line);
  }
  .row:last-child {
    border-bottom: none;
  }

  .destination {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 10px 4px;
    border: none;
    border-radius: 0;
    background: none;
    text-align: left;
    cursor: pointer;
  }
  .destination:hover:not(:disabled) {
    background: color-mix(in srgb, var(--line) 35%, transparent);
  }
  /* Locked while compressing: dimmed, so it's clear the choice can't change right now. */
  .destination:disabled,
  .tiers button:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .dot {
    width: 16px;
    height: 16px;
    flex: none;
    border: 1.5px solid var(--muted);
    border-radius: 50%;
    transition:
      border-width var(--medium) var(--ease),
      border-color var(--medium) var(--ease);
  }
  .selected .dot {
    border: 5px solid var(--accent);
  }
  .name {
    flex: 1;
  }
  .selected .name {
    font-weight: 600;
  }
  .hint {
    color: var(--muted);
  }

  .tiers {
    display: inline-flex;
    margin: -2px 0 10px 32px;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  .tiers button {
    padding: 3px 14px;
    border: none;
    border-radius: 4px;
    background: none;
    font-size: var(--text-sm);
    color: var(--muted);
    cursor: pointer;
  }
  .tiers button:hover:not(:disabled):not(.active) {
    color: var(--text);
  }
  .tiers button.active {
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .tiers button.remembered {
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--line);
  }
</style>
