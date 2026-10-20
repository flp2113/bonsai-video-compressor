//! Reads video metadata by running the bundled `ffprobe` sidecar.
//!
//! Split in two on purpose:
//! - `parse_ffprobe_json` is a *pure* function (text in, struct out) so it is easy to unit-test.
//! - `probe` is the Tauri command that does the I/O (runs ffprobe, reads file size).

use serde::{Deserialize, Serialize};
use tauri_plugin_shell::ShellExt;

/// What we send to the UI. `#[serde(rename_all = "camelCase")]` turns `size_bytes`
/// into `sizeBytes` in JSON, so it feels native on the TypeScript side.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub path: String,
    pub size_bytes: u64,
    pub duration_s: f64,
    /// Display size, i.e. already swapped for rotated phone videos.
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub bitrate_kbps: u32,
    /// `Option<T>` is Rust's "T or nothing" (like `T | null`); videos may have no audio.
    pub audio_codec: Option<String>,
    pub audio_bitrate_kbps: Option<u32>,
}

// --- Shapes of ffprobe's JSON output. Only the fields we need; serde ignores the rest. ---
// ffprobe prints most numbers as strings ("12.345"), hence the `String` types.

#[derive(Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Format,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    bit_rate: Option<String>,
    #[serde(default)]
    side_data_list: Vec<SideData>,
}

#[derive(Deserialize)]
struct SideData {
    rotation: Option<f64>,
}

#[derive(Deserialize)]
struct Format {
    duration: Option<String>,
    bit_rate: Option<String>,
}

/// Parses ffprobe's `"30000/1001"` frame-rate fractions. Returns `None` for `"0/0"` or garbage.
fn parse_fraction(s: &str) -> Option<f64> {
    let (num, den) = s.split_once('/')?; // `?` = "if this is None, return None right here"
    let (num, den): (f64, f64) = (num.parse().ok()?, den.parse().ok()?);
    if den == 0.0 || num == 0.0 { None } else { Some(num / den) }
}

fn parse_kbps(s: &Option<String>) -> Option<u32> {
    s.as_deref()?.parse::<f64>().ok().map(|bps| (bps / 1000.0).round() as u32)
}

pub fn parse_ffprobe_json(path: &str, json: &str, size_bytes: u64) -> Result<VideoInfo, String> {
    let out: FfprobeOutput =
        serde_json::from_str(json).map_err(|e| format!("Could not read ffprobe output: {e}"))?;

    let video = out
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"))
        .ok_or("This file has no video stream")?;
    let audio = out.streams.iter().find(|s| s.codec_type.as_deref() == Some("audio"));

    let duration_s: f64 = out
        .format
        .duration
        .as_deref()
        .and_then(|d| d.parse().ok())
        .filter(|d: &f64| *d > 0.0)
        .ok_or("Could not determine the video duration")?;

    let (mut width, mut height) = (video.width.unwrap_or(0), video.height.unwrap_or(0));
    // Phones store portrait video as landscape + a rotation flag; report what the viewer sees.
    let rotation = video.side_data_list.iter().find_map(|sd| sd.rotation).unwrap_or(0.0);
    if (rotation.abs() - 90.0).abs() < 1.0 || (rotation.abs() - 270.0).abs() < 1.0 {
        std::mem::swap(&mut width, &mut height);
    }

    let fps = video
        .avg_frame_rate
        .as_deref()
        .and_then(parse_fraction)
        .or_else(|| video.r_frame_rate.as_deref().and_then(parse_fraction))
        .unwrap_or(0.0);

    // Prefer the container's overall bitrate; otherwise derive it from size and duration.
    let bitrate_kbps = parse_kbps(&out.format.bit_rate)
        .unwrap_or_else(|| (size_bytes as f64 * 8.0 / duration_s / 1000.0).round() as u32);

    Ok(VideoInfo {
        path: path.to_string(),
        size_bytes,
        duration_s,
        width,
        height,
        fps,
        video_codec: video.codec_name.clone().unwrap_or_else(|| "unknown".into()),
        bitrate_kbps,
        audio_codec: audio.and_then(|a| a.codec_name.clone()),
        audio_bitrate_kbps: audio.and_then(|a| parse_kbps(&a.bit_rate)),
    })
}

/// Called from the UI with `invoke("probe", { path })`.
/// `async` so the UI stays responsive while ffprobe runs.
#[tauri::command]
pub async fn probe(app: tauri::AppHandle, path: String) -> Result<VideoInfo, String> {
    let size_bytes = std::fs::metadata(&path)
        .map_err(|e| format!("Cannot open file: {e}"))?
        .len();

    let output = app
        .shell()
        .sidecar("ffprobe")
        .map_err(|e| e.to_string())?
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", &path])
        .output()
        .await
        .map_err(|e| format!("Failed to run ffprobe: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Not a readable video file ({})", stderr.trim()));
    }

    parse_ffprobe_json(&path, &String::from_utf8_lossy(&output.stdout), size_bytes)
}

// `cfg(test)` = only compiled for `cargo test`, never shipped in the app.
#[cfg(test)]
mod tests {
    use super::*;

    const H264_AAC: &str = r#"{
        "streams": [
            {"codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
             "avg_frame_rate": "30000/1001", "r_frame_rate": "30000/1001", "bit_rate": "7800000"},
            {"codec_type": "audio", "codec_name": "aac", "bit_rate": "128000"}
        ],
        "format": {"duration": "12.500000", "bit_rate": "7950000"}
    }"#;

    #[test]
    fn parses_normal_video() {
        let info = parse_ffprobe_json("a.mp4", H264_AAC, 12_000_000).unwrap();
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert!((info.fps - 29.97).abs() < 0.01);
        assert_eq!(info.duration_s, 12.5);
        assert_eq!(info.video_codec, "h264");
        assert_eq!(info.bitrate_kbps, 7950);
        assert_eq!(info.audio_codec.as_deref(), Some("aac"));
        assert_eq!(info.audio_bitrate_kbps, Some(128));
    }

    #[test]
    fn handles_missing_audio_and_bitrate() {
        let json = r#"{
            "streams": [{"codec_type": "video", "codec_name": "vp9", "width": 640, "height": 360,
                         "avg_frame_rate": "0/0", "r_frame_rate": "25/1"}],
            "format": {"duration": "10.0"}
        }"#;
        let info = parse_ffprobe_json("b.webm", json, 1_250_000).unwrap();
        assert_eq!(info.audio_codec, None);
        assert_eq!(info.fps, 25.0); // fell back to r_frame_rate
        assert_eq!(info.bitrate_kbps, 1000); // 1.25 MB * 8 / 10 s
    }

    #[test]
    fn swaps_dimensions_for_rotated_phone_video() {
        let json = r#"{
            "streams": [{"codec_type": "video", "codec_name": "hevc", "width": 1920, "height": 1080,
                         "avg_frame_rate": "30/1", "side_data_list": [{"rotation": -90}]}],
            "format": {"duration": "5.0", "bit_rate": "10000000"}
        }"#;
        let info = parse_ffprobe_json("c.mov", json, 6_000_000).unwrap();
        assert_eq!((info.width, info.height), (1080, 1920));
    }

    #[test]
    fn rejects_audio_only_file() {
        let json = r#"{"streams": [{"codec_type": "audio", "codec_name": "mp3"}],
                       "format": {"duration": "180.0"}}"#;
        assert!(parse_ffprobe_json("song.mp3", json, 3_000_000).is_err());
    }
}
