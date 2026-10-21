<script lang="ts" module>
  /** Bar widths in % of the track. Linear scale, so the bars are honest about how much smaller the file gets. */
  export function meterLayout(
    originalBytes: number,
    capBytes: number | null,
    estimateBytes: number,
    progress: number | null,
    actualBytes: number | null,
  ) {
    const scale = Math.max(originalBytes, capBytes ?? 0, estimateBytes, actualBytes ?? 0) || 1;
    const result = actualBytes ?? estimateBytes;
    // While compressing, the "now" bar shrinks from the original size toward the result.
    const now =
      actualBytes !== null
        ? actualBytes
        : progress !== null
          ? originalBytes - (originalBytes - result) * (progress / 100)
          : originalBytes;
    return {
      now: (now / scale) * 100,
      result: (result / scale) * 100,
      cap: capBytes !== null ? (capBytes / scale) * 100 : null,
    };
  }
</script>

<script lang="ts">
  import { formatSize } from "$lib/format";

  interface Props {
    originalBytes: number;
    capBytes: number | null;
    estimateBytes: number;
    /** 0–100 while compressing, otherwise null. */
    progress?: number | null;
    /** The real output size once done. */
    actualBytes?: number | null;
  }
  let { originalBytes, capBytes, estimateBytes, progress = null, actualBytes = null }: Props = $props();

  const layout = $derived(meterLayout(originalBytes, capBytes, estimateBytes, progress, actualBytes));
  const done = $derived(actualBytes !== null);
</script>

<div
  class="meter"
  role="img"
  aria-label={done
    ? `Now ${formatSize(actualBytes!)}, was ${formatSize(originalBytes)}`
    : `${formatSize(originalBytes)} now, about ${formatSize(estimateBytes)} after compressing` +
      (capBytes !== null ? `, limit ${formatSize(capBytes)}` : "")}
>
  <div class="cap-row">
    {#if layout.cap !== null}
      <span class="cap-label" class:flip={layout.cap > 55} style:left="{layout.cap}%">
        {formatSize(capBytes!)} limit
      </span>
    {/if}
  </div>

  <div class="track">
    <div class="now" style:width="{layout.now}%"></div>
    <div class="result" style:width="{layout.result}%"></div>
    {#if layout.cap !== null}
      <div class="cap-tick" style:left="{layout.cap}%"></div>
    {/if}
  </div>

  <div class="labels">
    {#if done}
      <span class="muted">was {formatSize(originalBytes)}</span>
      <span class="result-label">{formatSize(actualBytes!)}</span>
    {:else}
      <span><span class="num">{formatSize(originalBytes)}</span> <span class="muted">now</span></span>
      <span><span class="muted">about</span> <span class="num result-label">{formatSize(estimateBytes)}</span></span>
    {/if}
  </div>
</div>

<style>
  .meter {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cap-row {
    position: relative;
    height: 16px;
  }
  .cap-label {
    position: absolute;
    bottom: 0;
    font-size: var(--text-sm);
    color: var(--muted);
    white-space: nowrap;
    transform: translateX(-1px);
  }
  /* Past the middle, hang the label to the left of the tick so it never runs off the edge. */
  .cap-label.flip {
    transform: translateX(calc(-100% + 1px));
  }

  .track {
    position: relative;
    height: 10px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--line) 45%, transparent);
  }
  .now,
  .result {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 5px;
    transition: width 0.35s cubic-bezier(0.2, 0.7, 0.2, 1);
  }
  .now {
    background: color-mix(in srgb, var(--muted) 38%, transparent);
  }
  .result {
    min-width: 4px;
    background: var(--accent);
  }
  .cap-tick {
    position: absolute;
    top: -6px;
    bottom: -3px;
    width: 2px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--text);
  }

  .labels {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-size: var(--text-lg);
  }
  .muted {
    font-size: var(--text-md);
    color: var(--muted);
  }
  .num {
    font-weight: 600;
  }
  .result-label {
    font-weight: 600;
    color: var(--accent);
  }
</style>
