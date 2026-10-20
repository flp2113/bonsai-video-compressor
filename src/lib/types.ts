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

export const VIDEO_EXTENSIONS = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "ts"];
