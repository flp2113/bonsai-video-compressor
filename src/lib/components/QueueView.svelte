<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { fileName, formatSize } from "$lib/format";
  import type { JobProgress, QueueItem } from "$lib/types";

  let { items }: { items: QueueItem[] } = $props();

  /** Latest progress per job id, from the same `job-progress` events the drop screen uses. */
  let progress = $state<Record<number, JobProgress>>({});
  $effect(() => {
    const unlisten = listen<JobProgress>("job-progress", ({ payload }) => {
      progress[payload.jobId] = payload;
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });

  const unfinished = $derived(items.some((i) => i.status.kind === "waiting" || i.status.kind === "running"));
  const presetLabels = $derived([...new Set(items.map((i) => i.presetLabel))]);
  const heading = $derived.by(() => {
    const verb = unfinished ? "Compressing" : "Compressed";
    if (presetLabels.length === 1) return `${verb} for ${presetLabels[0]}`;
    return `${verb} ${items.length} videos`;
  });
  const lastOutput = $derived.by(() => {
    for (const item of [...items].reverse()) if (item.status.kind === "done") return item.status.outputPath;
    return null;
  });
  const doneCount = $derived(items.filter((i) => i.status.kind === "done").length);

  function percentOf(item: QueueItem) {
    return item.jobId !== null ? (progress[item.jobId]?.percent ?? 0) : 0;
  }
</script>

<section class="queue">
  <header>
    <h1>{heading}</h1>
    <p class="muted">{doneCount} of {items.length} done</p>
  </header>

  <ul>
    {#each items as item (item.id)}
      {@const status = item.status}
      <li>
        <div class="file">
          <span class="name" title={item.path}>{fileName(item.path)}</span>
          {#if presetLabels.length > 1}
            <span class="muted small">{item.presetLabel}</span>
          {/if}
        </div>

        <div class="state">
          {#if status.kind === "waiting"}
            <span class="muted">Waiting</span>
          {:else if status.kind === "running"}
            <div class="bar" aria-hidden="true"><div style:width="{percentOf(item)}%"></div></div>
            <span class="percent">{Math.floor(percentOf(item))}%</span>
          {:else if status.kind === "done"}
            <span class="size">{formatSize(status.sizeBytes)}</span>
          {:else if status.kind === "failed"}
            <span class="error" title={status.message}>{status.message}</span>
          {:else}
            <span class="muted">Cancelled</span>
          {/if}
        </div>

        {#if status.kind === "waiting" || status.kind === "running"}
          <button
            class="cancel"
            aria-label="Cancel {fileName(item.path)}"
            onclick={() => invoke("cancel_queue_item", { id: item.id })}
          >
            <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
          </button>
        {:else}
          <span class="cancel-space"></span>
        {/if}
      </li>
    {/each}
  </ul>

  <footer>
    {#if unfinished}
      <button class="secondary wide" onclick={() => invoke("cancel_queue")}>Cancel all</button>
    {:else}
      {#if lastOutput}
        <button class="primary wide" onclick={() => lastOutput && revealItemInDir(lastOutput)}>Show in folder</button>
      {/if}
      <button class="secondary wide" onclick={() => invoke("clear_queue")}>Done</button>
    {/if}
  </footer>
</section>

<style>
  .queue {
    display: flex;
    flex-direction: column;
    gap: 16px;
    flex: 1;
    min-height: 0;
  }
  h1 {
    margin: 0;
    font-family: "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
    font-size: var(--text-xl);
    font-weight: 600;
    line-height: 1.2;
  }
  p {
    margin: 2px 0 0;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: var(--text-sm);
  }

  ul {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }
  li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 24px;
    align-items: center;
    gap: 12px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: none;
  }
  .file {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .state {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 170px;
  }
  .bar {
    width: 90px;
    height: 4px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s;
  }
  .percent {
    min-width: 34px;
    text-align: right;
  }
  .size {
    font-weight: 600;
    color: var(--accent);
  }
  .error {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    color: var(--danger);
  }

  .cancel {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .cancel:hover {
    background: color-mix(in srgb, var(--line) 50%, transparent);
    color: var(--text);
  }
  .cancel svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
  }

  footer {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .primary,
  .secondary {
    padding: 11px 20px;
    border-radius: var(--radius);
    cursor: pointer;
  }
  .wide {
    width: 100%;
  }
  .primary {
    border: none;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .primary:hover {
    background: color-mix(in srgb, var(--accent) 88%, var(--text));
  }
  .secondary {
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .secondary:hover {
    border-color: var(--muted);
  }
</style>
