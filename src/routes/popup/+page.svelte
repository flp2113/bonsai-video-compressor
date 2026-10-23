<script lang="ts">
  // The corner popup for right-click batches (window built by src-tauri/src/popup.rs).
  // Collapsed it's one card with the overall progress; the chevron expands a per-file list.
  // When the batch is done it turns into the summary, which hides itself after a few seconds.
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { batchTitle, fileName, formatSize } from "$lib/format";
  import type { JobProgress, QueueItem } from "$lib/types";

  /** How long a summary stays up when nothing failed. */
  const DISMISS_AFTER_MS = 6000;

  let items = $state<QueueItem[]>([]);
  let loaded = $state(false);
  /** Latest progress per job id, from the same `job-progress` events the main window uses. */
  let progress = $state<Record<number, JobProgress>>({});
  let expanded = $state(false);
  let hovering = $state(false);
  /** The failed row whose message is shown. */
  let openError = $state<number | null>(null);
  let copied = $state(false);
  let copyError = $state("");

  // Listen first, then fetch the current list, so nothing that changes in between is missed.
  $effect(() => {
    const unlisteners = [
      listen<QueueItem[]>("queue-changed", ({ payload }) => (items = payload)),
      listen<JobProgress>("job-progress", ({ payload }) => {
        progress[payload.jobId] = payload;
      }),
    ];
    Promise.all(unlisteners)
      .then(() => invoke<QueueItem[]>("get_queue"))
      .then((list) => {
        items = list;
        loaded = true;
      });
    return () => unlisteners.forEach((u) => u.then((fn) => fn()));
  });

  const busy = $derived(items.some((i) => i.status.kind === "waiting" || i.status.kind === "running"));
  const counted = $derived(items.filter((i) => i.status.kind !== "cancelled"));
  const doneItems = $derived(items.flatMap((i) => (i.status.kind === "done" ? [{ ...i, status: i.status }] : [])));
  const failedCount = $derived(items.filter((i) => i.status.kind === "failed").length);
  const finishedCount = $derived(doneItems.length + failedCount);
  const running = $derived(items.find((i) => i.status.kind === "running"));
  const presetLabels = $derived([...new Set(items.map((i) => i.presetLabel))].join(" · "));

  function percentOf(item: QueueItem) {
    return item.jobId !== null ? (progress[item.jobId]?.percent ?? 0) : 0;
  }

  /** Finished files count fully, the running one by its percent. Cancelled files don't count at all. */
  const overall = $derived(
    counted.length === 0 ? 0 : ((finishedCount + (running ? percentOf(running) / 100 : 0)) / counted.length) * 100,
  );

  const totalAfter = $derived(doneItems.reduce((sum, i) => sum + i.status.sizeBytes, 0));
  const totalBefore = $derived(doneItems.reduce((sum, i) => sum + (i.originalBytes ?? 0), 0));

  function runningText(item: QueueItem) {
    const p = item.jobId !== null ? progress[item.jobId] : undefined;
    const retry = p && p.pass > 1 ? ` (try ${p.pass})` : "";
    return `${Math.floor(p?.percent ?? 0)}%${retry}`;
  }

  // Keep the window exactly as tall as the card: Rust resizes it and keeps it pinned above the taskbar.
  let card = $state<HTMLElement>();
  let shown = false;
  $effect(() => {
    if (!card || !loaded) return;
    const observer = new ResizeObserver(async () => {
      if (!card) return;
      await invoke("resize_popup", { height: card.offsetHeight });
      if (!shown) {
        shown = true;
        invoke("popup_ready"); // first real layout: now the window may appear
      }
    });
    observer.observe(card);
    return () => observer.disconnect();
  });

  function dismiss() {
    invoke("dismiss_popup");
  }

  /** ✕: while working it only hides (the tray icon brings it back); afterwards it closes the summary. */
  function close() {
    if (busy) invoke("hide_popup");
    else dismiss();
  }

  // Emptied elsewhere (e.g. "Done" in the main window): nothing left to show.
  $effect(() => {
    if (loaded && items.length === 0) dismiss();
  });

  // A clean summary goes away by itself; hovering pauses it, and new files coming in cancel it.
  // If anything failed it stays until closed, so the error can be read.
  $effect(() => {
    if (!loaded || busy || items.length === 0 || failedCount > 0 || hovering) return;
    const timer = setTimeout(dismiss, DISMISS_AFTER_MS);
    return () => clearTimeout(timer);
  });

  async function copyAll() {
    try {
      await invoke("copy_files_to_clipboard", { paths: doneItems.map((i) => i.status.outputPath) });
      copied = true;
      copyError = "";
    } catch (e) {
      copyError = String(e);
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (hover only pauses the auto-hide timer) -->
<section
  class="card"
  bind:this={card}
  onmouseenter={() => (hovering = true)}
  onmouseleave={() => (hovering = false)}
  aria-live="polite"
>
  <header>
    <div class="title">
      {#if busy}
        <strong>{presetLabels}</strong>
        <span class="muted">{Math.min(finishedCount + 1, counted.length)} of {counted.length}</span>
      {:else}
        <strong class:error={doneItems.length === 0 && failedCount > 0}>{batchTitle(doneItems.length, failedCount)}</strong>
      {/if}
    </div>
    <button
      class="icon-button"
      aria-label={expanded ? "Hide files" : "Show files"}
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}
    >
      <svg class="chevron" class:open={expanded} viewBox="0 0 12 12" aria-hidden="true"><path d="M3 7.5 6 4.5 9 7.5" /></svg>
    </button>
    <button class="icon-button" aria-label={busy ? "Hide" : "Close"} title={busy ? "Hide (keeps compressing)" : "Close"} onclick={close}>
      <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
    </button>
  </header>

  {#if busy}
    <div class="bar" aria-hidden="true"><div style:width="{overall}%"></div></div>
    {#if running}
      <p class="now">
        <span class="name" title={running.path}>{fileName(running.path)}</span>
        <span class="muted">{runningText(running)}</span>
      </p>
    {:else}
      <p class="now muted">Reading video…</p>
    {/if}
  {:else if doneItems.length > 0}
    <p class="muted small">
      {#if totalBefore > 0}{`${formatSize(totalBefore)} → `}{/if}<span class="size">{formatSize(totalAfter)}</span>
      · {presetLabels}
    </p>
    <div class="pair">
      <button class="primary" onclick={copyAll}>
        {copied ? "Copied" : doneItems.length === 1 ? "Copy file" : "Copy files"}
      </button>
      <button class="secondary" onclick={() => revealItemInDir(doneItems[0].status.outputPath)}>Show in folder</button>
    </div>
    {#if copyError}<p class="error small">{copyError}</p>{/if}
  {:else if failedCount > 0 && !expanded}
    <button class="link small" onclick={() => (expanded = true)}>See what went wrong</button>
  {/if}

  {#if expanded}
    <ul>
      {#each items as item (item.id)}
        {@const status = item.status}
        <li>
          <span class="mark" aria-hidden="true">
            {#if status.kind === "done"}✓{:else if status.kind === "failed"}!{:else if status.kind === "running"}▸{:else}·{/if}
          </span>
          <span class="name" class:muted={status.kind === "cancelled"} title={item.path}>{fileName(item.path)}</span>
          <span class="state">
            {#if status.kind === "waiting"}
              <span class="muted">Waiting</span>
            {:else if status.kind === "running"}
              <span>{runningText(item)}</span>
            {:else if status.kind === "done"}
              <span class="size">{formatSize(status.sizeBytes)}</span>
            {:else if status.kind === "failed"}
              <button class="link error" aria-expanded={openError === item.id} onclick={() => (openError = openError === item.id ? null : item.id)}>
                Failed
              </button>
            {:else}
              <span class="muted">Cancelled</span>
            {/if}
          </span>
          {#if status.kind === "waiting" || status.kind === "running"}
            <button
              class="icon-button"
              aria-label="Cancel {fileName(item.path)}"
              onclick={() => invoke("cancel_queue_item", { id: item.id })}
            >
              <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
            </button>
          {:else}
            <span></span>
          {/if}
          {#if status.kind === "failed" && openError === item.id}
            <p class="message">{status.message}</p>
          {/if}
        </li>
      {/each}
    </ul>
    {#if busy}
      <button class="link small cancel-all" onclick={() => invoke("cancel_queue")}>Cancel all</button>
    {/if}
  {/if}
</section>

<style>
  /* The window is exactly the card: no page background or scrolling around it. */
  :global(html),
  :global(body) {
    height: auto;
    background: var(--surface);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 14px 16px 16px;
    user-select: none;
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: var(--text-sm);
  }
  .error {
    color: var(--danger);
  }
  .size {
    font-weight: 600;
    color: var(--accent);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .title {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex: 1;
    min-width: 0;
  }
  .title strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .title .muted {
    flex: none;
  }

  .bar {
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
  .now {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: var(--text-sm);
  }
  .now .muted {
    flex: none;
  }

  ul {
    max-height: 200px;
    margin: 0;
    padding: 6px 0 0;
    border-top: 1px solid var(--line);
    list-style: none;
    overflow-y: auto;
    font-size: var(--text-sm);
  }
  li {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr) auto 24px;
    align-items: center;
    gap: 8px;
    min-height: 28px;
  }
  .mark {
    color: var(--muted);
    text-align: center;
  }
  .message {
    grid-column: 2 / -1;
    padding-bottom: 6px;
    color: var(--danger);
    overflow-wrap: anywhere;
    user-select: text;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .primary,
  .secondary {
    padding: 6px 12px;
    border-radius: var(--radius);
    cursor: pointer;
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
    background: var(--bg);
  }
  .secondary:hover {
    border-color: var(--muted);
  }

  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    cursor: pointer;
    text-align: left;
  }
  .link.error {
    color: var(--danger);
  }
  .link:hover {
    text-decoration: underline;
  }
  .cancel-all {
    align-self: flex-start;
  }

  .icon-button {
    display: grid;
    place-items: center;
    flex: none;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }
  .icon-button:hover {
    background: color-mix(in srgb, var(--line) 50%, transparent);
    color: var(--text);
  }
  .icon-button svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .chevron {
    transition: transform 0.2s;
  }
  .chevron.open {
    transform: rotate(180deg);
  }
</style>
