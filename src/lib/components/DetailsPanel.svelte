<script lang="ts">
  import { formatDuration, formatFps, formatSize } from "$lib/format";
  import type { EncodePlan, VideoInfo } from "$lib/types";

  interface Props {
    id: string;
    open: boolean;
    info: VideoInfo;
    plan: EncodePlan | null;
    encoderLabel: string;
  }
  let { id, open, info, plan, encoderLabel }: Props = $props();

  const kbps = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)} Mbps` : `${n} kbps`);
</script>

<!-- The grid-rows trick animates to the content's natural height without measuring it. -->
<div {id} class="details" class:open inert={!open}>
  <div class="inner">
    <div class="columns">
      <div>
        <h3>Original</h3>
        <dl>
          <dt>Size</dt>
          <dd>{formatSize(info.sizeBytes)}</dd>
          <dt>Length</dt>
          <dd>{formatDuration(info.durationS)}</dd>
          <dt>Picture</dt>
          <dd>{info.width}×{info.height}, {formatFps(info.fps)}</dd>
          <dt>Video</dt>
          <dd>{info.videoCodec}, {kbps(info.bitrateKbps)}</dd>
          <dt>Audio</dt>
          <dd>
            {#if info.audioCodec}
              {info.audioCodec}{info.audioBitrateKbps ? `, ${kbps(info.audioBitrateKbps)}` : ""}
            {:else}
              None
            {/if}
          </dd>
        </dl>
      </div>

      {#if plan}
        <div>
          <h3>Output</h3>
          <dl>
            {#if plan.mode === "copy"}
              <dt>Method</dt>
              <dd>Copied, not re-encoded</dd>
            {:else}
              <dt>Picture</dt>
              <dd>{plan.width}×{plan.height}, {formatFps(plan.fps)}</dd>
              <dt>Video</dt>
              <dd>H.264, {kbps(plan.videoKbps)}</dd>
              <dt>Audio</dt>
              <dd>{plan.audioKbps > 0 ? `AAC, ${kbps(plan.audioKbps)}` : "None"}</dd>
              {#if encoderLabel}
                <dt>Encoder</dt>
                <dd>{encoderLabel}</dd>
              {/if}
            {/if}
          </dl>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .details {
    display: grid;
    grid-template-rows: 0fr;
    transition: grid-template-rows var(--medium) var(--ease);
  }
  .details.open {
    grid-template-rows: 1fr;
  }
  /* The text fades as the panel opens, so it doesn't look clipped while the height animates. */
  .inner {
    overflow: hidden;
    opacity: 0;
    transition: opacity var(--medium) linear;
  }
  .open .inner {
    opacity: 1;
  }
  .columns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    padding: 12px 0 4px;
    font-size: var(--text-sm);
  }
  h3 {
    margin: 0 0 4px;
    font-size: var(--text-sm);
    font-weight: 600;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 10px;
    margin: 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
