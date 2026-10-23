<script lang="ts">
  // The corner popup for right-click batches (window built by src-tauri/src/popup.rs).
  // Collapsed it's one card with the overall progress; the chevron expands a per-file list.
  // When the batch is done it turns into the summary, which fades away by itself after a few seconds.
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { fade, slide } from "svelte/transition";
  import StatusIcon from "$lib/components/StatusIcon.svelte";
  import { batchTitle, fileName, formatSize } from "$lib/format";
  import type { JobProgress, QueueItem } from "$lib/types";

  /** A clean summary stays fully visible this long, then fades out over `FADE_MS` (keep in sync with --slow). */
  const VISIBLE_MS = 5000;
  const FADE_MS = 1000;
  /** How long the card's own fade in/out takes (keep in sync with --medium). */
  const SHOW_HIDE_MS = 167;
  /** The window's transparent border around the card, on each side (room for the shadow). */
  const BORDER = 12;

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

  /** Whether the card is faded in. It starts out (invisible) and fades in once the window is on screen. */
  let shown = $state(false);
  /** The slow auto-hide fade is running. */
  let fading = $state(false);

  // Listen first, then fetch the current list, so nothing that changes in between is missed.
  $effect(() => {
    const unlisteners = [
      listen<QueueItem[]>("queue-changed", ({ payload }) => (items = payload)),
      listen<JobProgress>("job-progress", ({ payload }) => {
        progress[payload.jobId] = payload;
      }),
      // Rust showed the window again (tray click, or the batch finished while it was hidden).
      listen("popup-shown", () => fadeIn()),
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
  /** "Discord Free", or "Discord Free and WhatsApp" when two batches joined. */
  const presets = $derived(
    new Intl.ListFormat("en", { type: "conjunction" }).format([...new Set(items.map((i) => i.presetLabel))]),
  );

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
    const retry = p && p.pass > 1 ? `, try ${p.pass}` : "";
    return `${Math.floor(p?.percent ?? 0)}%${retry}`;
  }

  /** Two animation frames, so the browser has drawn the faded-out card before it fades back in. */
  function fadeIn() {
    shown = false;
    requestAnimationFrame(() => requestAnimationFrame(() => (shown = true)));
  }

  /** Fades the card out, then runs `then` (hide or dismiss the window). */
  function fadeOut(then: () => void) {
    shown = false;
    setTimeout(then, SHOW_HIDE_MS);
  }

  // Keep the window as tall as the card. Growing happens at once (the window is transparent, so the extra room
  // doesn't show and the card can animate into it); shrinking waits until the card has stopped changing size,
  // so the window doesn't resize on every frame of a collapse.
  let card = $state<HTMLElement>();
  let windowHeight = 0;
  let firstLayout = true;
  $effect(() => {
    if (!card || !loaded) return;
    let shrinkTimer: ReturnType<typeof setTimeout> | undefined;
    const resize = async (height: number) => {
      windowHeight = height;
      await invoke("resize_popup", { height });
    };
    const observer = new ResizeObserver(async () => {
      if (!card) return;
      const height = card.offsetHeight + 2 * BORDER;
      clearTimeout(shrinkTimer);
      if (height > windowHeight) await resize(height);
      else if (height < windowHeight) shrinkTimer = setTimeout(() => resize(height), 250);
      if (firstLayout) {
        firstLayout = false;
        await invoke("popup_ready"); // first real layout: the window may appear, and the card fades in
        fadeIn();
      }
    });
    observer.observe(card);
    return () => {
      observer.disconnect();
      clearTimeout(shrinkTimer);
    };
  });

  function dismiss() {
    fadeOut(() => invoke("dismiss_popup"));
  }

  /** ✕: while working it only hides (the tray icon brings it back); afterwards it closes the summary. */
  function close() {
    if (busy) fadeOut(() => invoke("hide_popup"));
    else dismiss();
  }

  // Emptied elsewhere (e.g. "Done" in the main window): nothing left to show.
  $effect(() => {
    if (loaded && items.length === 0) dismiss();
  });

  // A clean summary stays for a few seconds, then slowly fades out and closes. Hovering (even mid-fade) brings
  // it back and holds it; leaving starts the countdown again. If anything failed it stays until closed.
  // Timers, not `transitionend`: with Windows' animations turned off there's no transition to end.
  $effect(() => {
    fading = false;
    if (!loaded || !shown || busy || items.length === 0 || failedCount > 0 || hovering) return;
    const startFade = setTimeout(() => (fading = true), VISIBLE_MS);
    const closeAfterFade = setTimeout(() => invoke("dismiss_popup"), VISIBLE_MS + FADE_MS);
    return () => {
      clearTimeout(startFade);
      clearTimeout(closeAfterFade);
    };
  });

  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyAll() {
    try {
      await invoke("copy_files_to_clipboard", { paths: doneItems.map((i) => i.status.outputPath) });
      copied = true;
      copyError = "";
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 2000);
    } catch (e) {
      copyError = String(e);
    }
  }
</script>

<div class="frame" style:padding="{BORDER}px">
  <!-- svelte-ignore a11y_no_static_element_interactions (hover only holds the auto-hide) -->
  <section
    class="card"
    class:shown
    class:fading
    bind:this={card}
    onmouseenter={() => (hovering = true)}
    onmouseleave={() => (hovering = false)}
    aria-live="polite"
  >
    <header>
      <div class="title">
        {#if busy}
          <strong>{presets}</strong>
          <span class="muted">{Math.min(finishedCount + 1, counted.length)} of {counted.length}</span>
        {:else}
          <strong>{batchTitle(doneItems.length, failedCount)}</strong>
        {/if}
      </div>
      <button
        class="icon-button"
        title={expanded ? "Hide files" : "Show files"}
        aria-label={expanded ? "Hide files" : "Show files"}
        aria-expanded={expanded}
        onclick={() => (expanded = !expanded)}
      >
        <svg class="chevron" class:open={expanded} viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 7.5 6 4.5 9 7.5" />
        </svg>
      </button>
      <button
        class="icon-button"
        title={busy ? "Hide (keeps compressing)" : "Close"}
        aria-label={busy ? "Hide" : "Close"}
        onclick={close}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
      </button>
    </header>

    <!-- Working and summary share one grid cell, so they crossfade instead of jumping. -->
    <div class="state">
      {#if busy}
        <div class="layer" transition:fade={{ duration: SHOW_HIDE_MS }}>
          <div class="bar" aria-hidden="true"><div style:width="{overall}%"></div></div>
          <p class="now">
            {#if running}
              <span class="name" title={running.path}>{fileName(running.path)}</span>
              <span class="muted">{runningText(running)}</span>
            {:else}
              <span class="muted">Reading video…</span>
            {/if}
          </p>
        </div>
      {:else if doneItems.length > 0}
        <div class="layer" transition:fade={{ duration: SHOW_HIDE_MS }}>
          <p class="muted">
            <span class="size">{formatSize(totalAfter)}</span> for {presets}{#if totalBefore > 0}, down from
              {formatSize(totalBefore)}{/if}
          </p>
          <div class="pair">
            <button class="primary" onclick={copyAll}>
              {copied ? "Copied" : doneItems.length === 1 ? "Copy file" : "Copy files"}
            </button>
            <button class="secondary" onclick={() => revealItemInDir(doneItems[0].status.outputPath)}>
              Show in folder
            </button>
          </div>
          {#if copyError}<p class="error">{copyError}</p>{/if}
        </div>
      {:else if failedCount > 0 && !expanded}
        <div class="layer" transition:fade={{ duration: SHOW_HIDE_MS }}>
          <button class="link" onclick={() => (expanded = true)}>See what went wrong</button>
        </div>
      {/if}
    </div>

    {#if expanded}
      <div class="files" transition:slide={{ duration: SHOW_HIDE_MS }}>
        <ul>
          {#each items as item (item.id)}
            {@const status = item.status}
            <li>
              {#if status.kind === "failed"}
                <button
                  class="row clickable"
                  aria-expanded={openError === item.id}
                  onclick={() => (openError = openError === item.id ? null : item.id)}
                >
                  <StatusIcon kind={status.kind} />
                  <span class="name" title={item.path}>{fileName(item.path)}</span>
                  <span class="error">Failed</span>
                  <svg class="chevron small" class:open={openError === item.id} viewBox="0 0 12 12" aria-hidden="true">
                    <path d="M3 4.5 6 7.5 9 4.5" />
                  </svg>
                </button>
                {#if openError === item.id}
                  <p class="message" transition:slide={{ duration: SHOW_HIDE_MS }}>{status.message}</p>
                {/if}
              {:else}
                <div class="row">
                  <StatusIcon kind={status.kind} />
                  <span class="name" class:muted={status.kind === "cancelled"} title={item.path}>
                    {fileName(item.path)}
                  </span>
                  {#if status.kind === "waiting"}
                    <span class="muted">Waiting</span>
                  {:else if status.kind === "running"}
                    <span class="muted">{runningText(item)}</span>
                  {:else if status.kind === "done"}
                    <span class="size">{formatSize(status.sizeBytes)}</span>
                  {:else}
                    <span class="muted">Cancelled</span>
                  {/if}
                  {#if status.kind === "waiting" || status.kind === "running"}
                    <button
                      class="icon-button"
                      title="Cancel"
                      aria-label="Cancel {fileName(item.path)}"
                      onclick={() => invoke("cancel_queue_item", { id: item.id })}
                    >
                      <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M3 3l6 6M9 3l-6 6" /></svg>
                    </button>
                  {:else}
                    <span></span>
                  {/if}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
        {#if busy}
          <button class="link cancel-all" onclick={() => invoke("cancel_queue")}>Cancel all</button>
        {/if}
      </div>
    {/if}
  </section>
</div>

<style>
  /* The window is transparent; only the card is drawn. `:root` outranks theme.css's `html, body` background
     whatever order the stylesheets load in. */
  :global(:root),
  :global(:root body) {
    background: transparent;
  }

  /* The card sits at the bottom, so when it grows the window has already made room above it. */
  .frame {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    height: 100vh;
  }

  /* Windows 11 flyout: solid surface, hairline border, a soft shadow for elevation. */
  .card {
    --popup-shadow: 0 2px 4px rgb(0 0 0 / 0.06), 0 4px 10px rgb(0 0 0 / 0.14);
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 12px 14px 16px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    box-shadow: var(--popup-shadow);
    font-size: var(--text-sm);
    user-select: none;
    opacity: 0;
    transform: translateY(8px);
    transition:
      opacity var(--medium) var(--ease),
      transform var(--medium) var(--ease);
  }
  .card.shown {
    opacity: 1;
    transform: none;
  }
  /* The auto-hide: a slow, even fade. Removing the class (hover) brings it back at the normal speed. */
  .card.shown.fading {
    opacity: 0;
    transition: opacity var(--slow) linear;
  }
  @media (prefers-color-scheme: dark) {
    .card {
      --popup-shadow: 0 2px 4px rgb(0 0 0 / 0.2), 0 4px 10px rgb(0 0 0 / 0.32);
    }
  }

  p {
    margin: 0;
  }
  .muted {
    color: var(--muted);
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
    margin-right: -4px;
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
    font-size: var(--text-md);
    font-weight: 600;
  }
  .title .muted {
    flex: none;
  }

  .state {
    display: grid;
  }
  .state:empty {
    display: none;
  }
  .layer {
    grid-area: 1 / 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .bar {
    height: 3px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--line) 70%, transparent);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.4s var(--ease);
  }
  .now {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .now .muted {
    flex: none;
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .pair button {
    height: 32px;
    padding: 0 12px;
    font-size: var(--text-sm);
  }
  .link {
    align-self: flex-start;
    text-align: left;
  }

  .files {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 6px;
    border-top: 1px solid var(--line);
  }
  ul {
    max-height: 196px;
    margin: 0 -6px;
    padding: 0;
    list-style: none;
    overflow-y: auto;
  }
  .row {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr) auto 24px;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 28px;
    padding: 0 0 0 6px;
    border: none;
    border-radius: 4px;
    background: none;
    font-size: var(--text-sm);
    text-align: left;
  }
  .row.clickable {
    cursor: pointer;
  }
  .row.clickable:hover {
    background: color-mix(in srgb, var(--line) 50%, transparent);
  }
  .message {
    padding: 2px 6px 8px 28px;
    color: var(--danger);
    overflow-wrap: anywhere;
    user-select: text;
  }

  .chevron {
    transition: transform var(--medium) var(--ease);
  }
  .chevron.open {
    transform: rotate(180deg);
  }
  .chevron.small {
    justify-self: center;
    width: 12px;
    height: 12px;
    fill: none;
    stroke: var(--muted);
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
