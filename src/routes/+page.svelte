<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open } from "@tauri-apps/plugin-dialog";
  import { VIDEO_EXTENSIONS, type EncodePlan, type PresetSummary, type VideoInfo } from "$lib/types";

  let info = $state<VideoInfo | null>(null);
  let error = $state("");
  let loading = $state(false);
  let dragging = $state(false);

  let presets = $state<PresetSummary[]>([]);
  let presetId = $state("discord-free");
  let plan = $state<EncodePlan | null>(null);
  let planError = $state("");

  invoke<PresetSummary[]>("list_presets").then((list) => (presets = list));

  async function load(path: string) {
    loading = true;
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
  const megabytes = (bytes: number) => (bytes / 1024 / 1024).toFixed(1) + " MB";
  function duration(s: number) {
    const m = Math.floor(s / 60);
    const sec = (s % 60).toFixed(1).padStart(4, "0");
    return `${m}:${sec}`;
  }
</script>

<main>
  <button class="drop" class:dragging onclick={chooseFile} disabled={loading}>
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
        <button class:selected={preset.id === presetId} onclick={() => (presetId = preset.id)}>
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

  .plan p {
    margin: 0;
  }
  .plan .warning {
    margin-top: 6px;
    color: var(--muted);
    font-size: 12px;
  }
</style>
