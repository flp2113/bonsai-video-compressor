import type { EncodePlan, PresetSummary, VideoInfo } from "./types";

export const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/** Decimal units (1 MB = 1,000,000 bytes) to match how the presets define their limits. */
export function formatSize(bytes: number) {
  const mb = bytes / 1_000_000;
  if (mb >= 1000) return `${(mb / 1000).toFixed(1)} GB`;
  if (mb >= 100) return `${Math.round(mb)} MB`;
  return `${mb.toFixed(1)} MB`;
}

/** 75.4 → "1:15" */
export function formatDuration(seconds: number) {
  const s = Math.floor(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

/** 60 → "60 fps", 29.97 → "29.97 fps" */
export function formatFps(fps: number) {
  const rounded = Math.round(fps * 100) / 100;
  return `${Number.isInteger(rounded) ? rounded : rounded.toFixed(2)} fps`;
}

/** One plain sentence about what the plan gives up, e.g. "Lowered to 720p and 30 fps to fit 20 MB". */
export function planSentence(plan: EncodePlan, info: VideoInfo, preset: PresetSummary | undefined) {
  if (plan.mode === "copy") return "Already fits, so it's copied as-is with no quality loss";

  const changes: string[] = [];
  const outShort = Math.min(plan.width, plan.height);
  if (outShort < Math.min(info.width, info.height)) changes.push(`${outShort}p`);
  if (plan.targetFps !== null) changes.push(`${Math.round(plan.targetFps)} fps`);

  if (changes.length === 0) return "Keeps the full resolution and frame rate";
  const fit = preset?.capBytes ? ` to fit ${preset.hint}` : ` for ${preset?.group ?? "this destination"}`;
  return `Lowered to ${changes.join(" and ")}${fit}`;
}
