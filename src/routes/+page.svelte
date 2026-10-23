<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { fade } from "svelte/transition";
  import DestinationPicker from "$lib/components/DestinationPicker.svelte";
  import DetailsPanel from "$lib/components/DetailsPanel.svelte";
  import DropOverlay from "$lib/components/DropOverlay.svelte";
  import QueueView from "$lib/components/QueueView.svelte";
  import SizeMeter from "$lib/components/SizeMeter.svelte";
  import { fileName, formatDuration, formatFps, planSentence } from "$lib/format";
  import {
    VIDEO_EXTENSIONS,
    type EncodePlan,
    type JobDone,
    type JobError,
    type JobProgress,
    type PresetSummary,
    type QueueItem,
    type VideoInfo,
  } from "$lib/types";

  const PRESET_KEY = "bonsai.presetId";

  let info = $state<VideoInfo | null>(null);
  let error = $state("");
  let loading = $state(false);
  let dragging = $state(false);
  let detailsOpen = $state(false);

  let presets = $state<PresetSummary[]>([]);
  let presetId = $state(readSavedPreset() ?? "discord-free");
  let plan = $state<EncodePlan | null>(null);
  let planError = $state("");

  let job = $state<JobProgress | null>(null);
  let done = $state<JobDone | null>(null);
  let jobError = $state("");
  let jobCancelled = $state(false);

  let encoderLabel = $state("");

  /** Files sent from Explorer's right-click menu (src-tauri/src/queue.rs). While non-empty, the queue is shown. */
  let queue = $state<QueueItem[]>([]);
  const queueBusy = $derived(queue.some((i) => i.status.kind === "waiting" || i.status.kind === "running"));

  const preset = $derived(presets.find((p) => p.id === presetId));
  /** Which screen is showing; the window fades it in when this changes. */
  const screen = $derived(queue.length > 0 ? "queue" : info ? info.path : "drop");

  invoke<PresetSummary[]>("list_presets").then((list) => {
    presets = list;
    if (!list.some((p) => p.id === presetId)) presetId = list[0]?.id ?? presetId;
  });
  invoke<string>("get_encoder").then((label) => (encoderLabel = label));

  // The window may be created after files were queued: start listening first, then fetch the current list,
  // so nothing added in between is missed.
  $effect(() => {
    const unlisten = listen<QueueItem[]>("queue-changed", ({ payload }) => (queue = payload));
    unlisten.then(() => invoke<QueueItem[]>("get_queue")).then((items) => (queue = items));
    return () => {
      unlisten.then((fn) => fn());
    };
  });

  // localStorage can be unavailable or throw (e.g. blocked storage); the app works fine without it.
  function readSavedPreset() {
    try {
      return localStorage.getItem(PRESET_KEY);
    } catch {
      return null;
    }
  }

  function selectPreset(id: string) {
    presetId = id;
    done = null;
    clearJobMessages();
    try {
      localStorage.setItem(PRESET_KEY, id);
    } catch {}
  }

  function clearJobMessages() {
    jobError = "";
    jobCancelled = false;
  }

  async function load(path: string) {
    if (job || queueBusy) return; // don't swap the video out from under a running job
    loading = true;
    done = null;
    clearJobMessages();
    error = "";
    info = null;
    try {
      info = await invoke<VideoInfo>("probe", { path });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Re-plan whenever the video or the selected preset changes.
  $effect(() => {
    plan = null;
    planError = "";
    if (!info) return;
    let stale = false;
    invoke<EncodePlan>("plan_compression", { info, presetId })
      .then((p) => !stale && (plan = p))
      .catch((e) => !stale && (planError = String(e)));
    return () => {
      stale = true;
    };
  });

  async function compress() {
    if (!info) return;
    done = null;
    clearJobMessages();
    try {
      const jobId = await invoke<number>("compress", { info, presetId });
      job = { jobId, percent: 0, speed: null, pass: 1 };
      // A very fast job (e.g. a copy) may have finished before we even learned its id.
      earlyFinish.get(jobId)?.();
      earlyFinish.clear();
    } catch (e) {
      jobError = String(e);
    }
  }

  let recopied = $state(false);
  let copyError = $state("");
  /** Briefly true after "Copy file" is clicked, so the button confirms it. */
  let justCopied = $state(false);
  let justCopiedTimer: ReturnType<typeof setTimeout> | undefined;

  /** Puts the finished file on the clipboard again (e.g. after copying something else meanwhile). */
  async function copyDone() {
    if (!done) return;
    try {
      await invoke("copy_files_to_clipboard", { paths: [done.outputPath] });
      recopied = true;
      copyError = "";
      justCopied = true;
      clearTimeout(justCopiedTimer);
      justCopiedTimer = setTimeout(() => (justCopied = false), 2000);
    } catch (e) {
      copyError = String(e);
    }
  }

  function cancel() {
    if (job) invoke("cancel", { jobId: job.jobId });
  }

  const earlyFinish = new Map<number, () => void>();

  /** Runs `apply` for the current job, or keeps it until `compress()` learns the job id. */
  function onFinish(jobId: number, apply: () => void) {
    const run = () => {
      job = null;
      apply();
    };
    if (job?.jobId === jobId) run();
    else earlyFinish.set(jobId, run);
  }

  function onJobError(payload: JobError) {
    if (payload.cancelled) jobCancelled = true;
    else jobError = payload.message;
  }

  // Job events come from Rust (src-tauri/src/jobs.rs).
  $effect(() => {
    const unlisteners = [
      listen<JobProgress>("job-progress", ({ payload }) => {
        if (job?.jobId === payload.jobId) job = payload;
      }),
      listen<JobDone>("job-done", ({ payload }) =>
        onFinish(payload.jobId, () => {
          done = payload;
          recopied = false;
          copyError = "";
        }),
      ),
      listen<JobError>("job-error", ({ payload }) => onFinish(payload.jobId, () => onJobError(payload))),
    ];
    return () => unlisteners.forEach((u) => u.then((fn) => fn()));
  });

  async function chooseFile() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Video", extensions: VIDEO_EXTENSIONS }],
    });
    if (path) load(path);
  }

  // HTML5 drop events don't expose real file paths, so listen to Tauri's native drag-drop instead.
  $effect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "enter" || payload.type === "over") dragging = true;
      else if (payload.type === "leave") dragging = false;
      else if (payload.type === "drop") {
        dragging = false;
        if (payload.paths.length > 0) load(payload.paths[0]);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  });

  function progressText(p: JobProgress) {
    const what = p.pass > 1 ? `Slightly over the limit, compressing again (try ${p.pass})` : "Compressing";
    const speed = p.speed ? `, ${p.speed.toFixed(1)}× real time` : "";
    return `${what}… ${Math.floor(p.percent)}%${speed}`;
  }
</script>

{#if dragging && !job && !queueBusy}
  <DropOverlay />
{/if}

<!-- Each screen fades in when you get to it (a dropped video, a right-click batch, back to the drop screen). -->
{#key screen}
<main in:fade={{ duration: 167 }}>
  {#if queue.length > 0}
    <QueueView items={queue} />
  {:else if !info}
    <section class="empty">
      {#if loading}
        <h1>Reading video…</h1>
      {:else}
        <h1>Drop a video</h1>
        <p class="muted">Bonsai shrinks it to fit Discord, X or WhatsApp.</p>
        <button class="secondary" onclick={chooseFile}>Choose a file</button>
        {#if error}
          <p class="error">{error}</p>
        {/if}
      {/if}
    </section>
  {:else}
    <header>
      <div class="name-row">
        <h1 title={info.path}>{fileName(info.path)}</h1>
        <button class="link" onclick={chooseFile} disabled={job !== null}>Change</button>
      </div>
      <div class="meta-row">
        <span class="muted">
          {formatDuration(info.durationS)} · {info.width}×{info.height} · {formatFps(info.fps)}
        </span>
        <button
          class="link"
          aria-expanded={detailsOpen}
          aria-controls="details"
          onclick={() => (detailsOpen = !detailsOpen)}
        >
          Details
          <svg class="chevron" class:open={detailsOpen} viewBox="0 0 12 12" aria-hidden="true">
            <path d="M3 4.5 6 7.5 9 4.5" />
          </svg>
        </button>
      </div>
      <DetailsPanel id="details" open={detailsOpen} {info} {plan} {encoderLabel} />
    </header>

    <DestinationPicker {presets} {presetId} disabled={job !== null} onselect={selectPreset} />

    <section class="outcome" aria-live="polite">
      {#if planError}
        <p class="error">{planError}</p>
      {:else if plan}
        <SizeMeter
          originalBytes={info.sizeBytes}
          capBytes={preset?.capBytes ?? null}
          estimateBytes={plan.estimatedBytes}
          progress={job?.percent ?? null}
          actualBytes={done?.sizeBytes ?? null}
        />
        {#if job}
          <p class="status" in:fade={{ duration: 167 }}>{progressText(job)}</p>
        {:else if done}
          <p class="status" in:fade={{ duration: 167 }}>
            Saved as <strong>{fileName(done.outputPath)}</strong>
            <span class="muted">
              {done.copied || recopied ? "On the clipboard, paste it anywhere" : copyError || "Ready to share"}
            </span>
          </p>
        {:else}
          <p class="status muted" in:fade={{ duration: 167 }}>{planSentence(plan, info, preset)}</p>
        {/if}
        {#each plan.warnings as warning}
          <p class="warning">{warning}</p>
        {/each}
      {/if}
      {#if jobError}
        <p class="error">{jobError}</p>
      {:else if jobCancelled}
        <p class="muted">Cancelled. Nothing was saved.</p>
      {/if}
    </section>

    <footer>
      {#if job}
        <button class="secondary wide" onclick={cancel} in:fade={{ duration: 167 }}>Cancel</button>
      {:else if done}
        <div class="pair" in:fade={{ duration: 167 }}>
          <!-- Already copied automatically, so copying again is the secondary action. -->
          <button class="primary" onclick={() => done && revealItemInDir(done.outputPath)}>Show in folder</button>
          <button class="secondary" onclick={copyDone}>{justCopied ? "Copied" : "Copy file"}</button>
        </div>
      {:else}
        <button class="primary wide" onclick={compress} disabled={!plan} in:fade={{ duration: 167 }}>Compress</button>
      {/if}
    </footer>
  {/if}
</main>
{/key}

<style>
  main {
    display: flex;
    flex-direction: column;
    gap: 20px;
    height: 100vh;
    padding: var(--gutter);
    overflow-y: auto;
  }

  h1 {
    margin: 0;
    font-family: "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
    font-size: var(--text-xl);
    font-weight: 600;
    line-height: 1.2;
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
  .warning {
    font-size: var(--text-sm);
    color: var(--warn);
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    margin: auto 0;
    padding-bottom: 40px;
  }
  .empty .secondary {
    margin-top: 12px;
  }

  header {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name-row,
  .meta-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .name-row h1 {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The shared .link (theme.css), laid out for the header rows. */
  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: none;
    padding: 2px 4px;
    margin-right: -4px;
    border-radius: 4px;
  }
  .chevron {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform var(--medium) var(--ease);
  }
  .chevron.open {
    transform: rotate(180deg);
  }

  .outcome {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .status {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .status strong {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .status .muted {
    font-size: var(--text-sm);
  }

  footer {
    margin-top: auto;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .pair button {
    padding-block: 11px;
  }
</style>
