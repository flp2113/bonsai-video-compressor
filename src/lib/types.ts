/** Mirrors `VideoInfo` in src-tauri/src/probe.rs (serde renames fields to camelCase). */
export interface VideoInfo {
  path: string;
  sizeBytes: number;
  durationS: number;
  width: number;
  height: number;
  fps: number;
  videoCodec: string;
  bitrateKbps: number;
  audioCodec: string | null;
  audioBitrateKbps: number | null;
}

/** Mirrors `PresetSummary` in src-tauri/src/presets.rs. */
export interface PresetSummary {
  id: string;
  label: string;
  /** Destination row in the list, e.g. "Discord". */
  group: string;
  /** Option within a group, e.g. "Nitro"; null when the group has a single preset. */
  tier: string | null;
  /** Size limit in bytes. */
  capBytes: number;
  /** The size limit as text, e.g. "20 MB". */
  hint: string;
}

/** Mirrors `EncodePlan` in src-tauri/src/planner.rs. */
export interface EncodePlan {
  mode: "copy" | "encode";
  width: number;
  height: number;
  fps: number;
  targetFps: number | null;
  videoKbps: number;
  audioKbps: number;
  estimatedBytes: number;
  /** Things to know before sending, e.g. X's 2:20 limit for free accounts. */
  warnings: string[];
}

/** Payloads of the events emitted by src-tauri/src/jobs.rs. */
export interface JobProgress {
  jobId: number;
  percent: number;
  speed: number | null;
  /** 1 for the first encode; 2-3 while re-encoding to fit the size limit. */
  pass: number;
}

export interface JobDone {
  jobId: number;
  outputPath: string;
  sizeBytes: number;
  /** e.g. "NVIDIA NVENC", or "Copied" when nothing was re-encoded. */
  encoder: string;
  /** Whether the file was put on the clipboard (single-file jobs). */
  copied: boolean;
}

export interface JobError {
  jobId: number;
  message: string;
  cancelled: boolean;
}

/** Mirrors `QueueStatus` in src-tauri/src/queue.rs (serde `tag = "kind"`). */
export type QueueStatus =
  | { kind: "waiting" }
  | { kind: "running" }
  | { kind: "done"; outputPath: string; sizeBytes: number }
  | { kind: "failed"; message: string }
  | { kind: "cancelled" };

/** Mirrors `QueueItem` in src-tauri/src/queue.rs: one file sent from Explorer's right-click menu. */
export interface QueueItem {
  id: number;
  path: string;
  presetId: string;
  presetLabel: string;
  status: QueueStatus;
  /** Set once compressing has started; matches `JobProgress.jobId`. */
  jobId: number | null;
}

/** Keep in sync with `VIDEO_EXTENSIONS` in src-tauri/src/shell_menu.rs. */
export const VIDEO_EXTENSIONS = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "ts"];
