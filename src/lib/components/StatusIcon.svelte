<script lang="ts">
  // One small icon per queue status, shared by the popup and the main window's queue.
  import type { QueueStatus } from "$lib/types";

  let { kind }: { kind: QueueStatus["kind"] } = $props();
</script>

<svg class="status {kind}" viewBox="0 0 16 16" aria-hidden="true">
  {#if kind === "done"}
    <path d="M3.5 8.5 6.5 11.5 12.5 4.5" />
  {:else if kind === "failed"}
    <circle cx="8" cy="8" r="6" />
    <path d="M8 4.75v3.75M8 11.25v.01" />
  {:else if kind === "running"}
    <!-- Like Windows' ProgressRing: a short arc turning on a faint ring. -->
    <circle class="ring" cx="8" cy="8" r="5.5" />
    <path d="M8 2.5a5.5 5.5 0 0 1 5.5 5.5" />
  {:else if kind === "waiting"}
    <circle class="dot" cx="8" cy="8" r="2" />
  {:else}
    <path d="M5 8h6" />
  {/if}
</svg>

<style>
  .status {
    width: 14px;
    height: 14px;
    flex: none;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
    color: var(--muted);
  }
  .done,
  .running {
    color: var(--accent);
  }
  .failed {
    color: var(--danger);
  }
  .ring {
    stroke: var(--line);
  }
  .dot {
    fill: currentColor;
    stroke: none;
  }
  .running {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
