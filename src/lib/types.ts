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
  hint: string;
}

/** Mirrors `EncodePlan` in src-tauri/src/planner.rs. */
export interface EncodePlan {
  mode: "copy" | "encode";
  width: number;
  height: number;
  fps: number;
  videoKbps: number;
  audioKbps: number;
  estimatedBytes: number;
  warnings: string[];
}

export const VIDEO_EXTENSIONS = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "ts"];
