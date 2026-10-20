<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import {
    VIDEO_EXTENSIONS,
    type EncodePlan,
    type JobDone,
    type JobError,
    type JobProgress,
    type PresetSummary,
    type VideoInfo,
  } from "$lib/types";

  let info = $state<VideoInfo | null>(null);
  let error = $state("");
  let loading = $state(false);
  let dragging = $state(false);

  let presets = $state<PresetSummary[]>([]);
  let presetId = $state("discord-free");
  let plan = $state<EncodePlan | null>(null);
  let planError = $state("");

  let job = $state<JobProgress | null>(null);
  let done = $state<JobDone | null>(null);
  let jobError = $state("");

  invoke<PresetSummary[]>("list_presets").then((list) => (presets = list));

  async function load(path: string) {
    if (job) return; // don't swap the video out from under a running job
    loading = true;
    done = null;
    jobError = "";
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
    jobError = "";
    try {
      const jobId = await invoke<number>("compress", { info, presetId });
      job = { jobId, percent: 0, speed: null };
      // A very fast job (e.g. a copy) may have finished before we even learned its id.
      earlyFinish.get(jobId)?.();
      earlyFinish.clear();
    } catch (e) {
      jobError = String(e);
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

  // Job events come from Rust (src-tauri/src/jobs.rs).
  $effect(() => {
    const unlisteners = [
      listen<JobProgress>("job-progress", ({ payload }) => {
        if (job?.jobId === payload.jobId) job = payload;
      }),
      listen<JobDone>("job-done", ({ payload }) => onFinish(payload.jobId, () => (done = payload))),
      listen<JobError>("job-error", ({ payload }) => onFinish(payload.jobId, () => (jobError = payload.message))),
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

  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  // Decimal MB (1,000,000 bytes) to match how the presets define their limits.
  const megabytes = (bytes: number) => (bytes / 1_000_000).toFixed(1) + " MB";
  function duration(s: number) {
    const m = Math.floor(s / 60);
    const sec = (s % 60).toFixed(1).padStart(4, "0");
    return `${m}:${sec}`;
  }
</script>

<main>
  <button class="drop" class:dragging onclick={chooseFile} disabled={loading || job !== null}>
    {#if loading}
      Reading video…
    {:else}
      <strong>Drop a video here</strong>
      <span>or click to choose a file</span>
    {/if}
  </button>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if info}
    <section class="card">
      <h2 title={info.path}>{fileName(info.path)}</h2>
      <dl>
        <dt>Size</dt><dd>{megabytes(info.sizeBytes)}</dd>
        <dt>Duration</dt><dd>{duration(info.durationS)}</dd>
        <dt>Resolution</dt><dd>{info.width}×{info.height}</dd>
        <dt>Frame rate</dt><dd>{info.fps.toFixed(2)} fps</dd>
        <dt>Video</dt><dd>{info.videoCodec} · {info.bitrateKbps} kbps total</dd>
        <dt>Audio</dt>
        <dd>
          {#if info.audioCodec}
            {info.audioCodec}{info.audioBitrateKbps ? ` · ${info.audioBitrateKbps} kbps` : ""}
          {:else}
            none
          {/if}
        </dd>
      </dl>
    </section>

    <section class="presets" aria-label="Presets">
      {#each presets as preset (preset.id)}
        <button
          class:selected={preset.id === presetId}
          disabled={job !== null}
          onclick={() => (presetId = preset.id)}
        >
          <strong>{preset.label}</strong>
          <span>{preset.hint}</span>
        </button>
      {/each}
    </section>

    {#if planError}
      <p class="error">{planError}</p>
    {:else if plan}
      <section class="card plan">
        {#if plan.mode === "copy"}
          <p>Already fits: will be copied without re-encoding ({megabytes(plan.estimatedBytes)}).</p>
        {:else}
          <p>
            {plan.width}×{plan.height} · {Math.round(plan.fps)} fps ·
            {(plan.videoKbps / 1000).toFixed(2)} Mbps · ≈{megabytes(plan.estimatedBytes)}
          </p>
        {/if}
        {#each plan.warnings as warning}
          <p class="warning">{warning}</p>
        {/each}
      </section>

      {#if job}
        <section class="job">
          <div class="bar"><div style:width="{job.percent}%"></div></div>
          <span>{Math.floor(job.percent)}%{job.speed ? ` · ${job.speed.toFixed(1)}×` : ""}</span>
          <button class="secondary" onclick={cancel}>Cancel</button>
        </section>
      {:else}
        <button class="primary" onclick={compress}>Compress</button>
      {/if}
    {/if}

    {#if jobError}
      <p class="error">{jobError}</p>
    {/if}

    {#if done}
      <section class="card done">
        <p>
          Saved <strong>{fileName(done.outputPath)}</strong> · {megabytes(done.sizeBytes)}
        </p>
        {#if done.overCap}
          <p class="warning">Came out slightly over the size limit. Automatic retry is coming in the next update.</p>
        {/if}
        <button class="secondary" onclick={() => done && revealItemInDir(done.outputPath)}>Show in folder</button>
      </section>
    {/if}
  {/if}
</main>

<style>
  :global(:root) {
    --bg: #f6f6f4;
    --surface: #ffffff;
    --text: #1b1b1a;
    --muted: #6b6b66;
    --border: #d9d9d4;
    --accent: #3f7d4e;
    --accent-soft: #e3efe5;
    --danger: #b3261e;
    font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif;
    font-size: 14px;
    color: var(--text);
    background: var(--bg);
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #1c1c1b;
      --surface: #262625;
      --text: #ececea;
      --muted: #9a9a94;
      --border: #3a3a38;
      --accent: #7cc28c;
      --accent-soft: #2a3a2d;
      --danger: #f2b8b5;
    }
  }
  :global(body) {
    margin: 0;
    background: var(--bg);
  }

  main {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
  }

  .drop {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 160px;
    border: 2px dashed var(--border);
    border-radius: 12px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
  }
  .drop span {
    color: var(--muted);
  }
  .drop:hover,
  .drop.dragging {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .drop:disabled {
    cursor: progress;
  }

  .error {
    margin: 0;
    color: var(--danger);
  }

  .card {
    padding: 12px 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
  }
  .card h2 {
    margin: 0 0 8px;
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 16px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }

  .presets {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .presets button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .presets button span {
    color: var(--muted);
  }
  .presets button.selected {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .presets button:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .plan p,
  .done p {
    margin: 0;
  }
  .done {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  .primary,
  .secondary {
    padding: 8px 16px;
    border-radius: 8px;
    font: inherit;
    cursor: pointer;
  }
  .primary {
    border: none;
    background: var(--accent);
    color: var(--bg);
    font-weight: 600;
  }
  .secondary {
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
  }

  .job {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .job span {
    min-width: 72px;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    flex: 1;
    height: 8px;
    border-radius: 4px;
    background: var(--border);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .plan .warning {
    margin-top: 6px;
    color: var(--muted);
    font-size: 12px;
  }
</style>
