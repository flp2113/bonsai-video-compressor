//! Decides *how* to compress a video for a preset: copy or re-encode, output size, frame rate, bitrates.
//!
//! Everything here is pure math on `VideoInfo` (no ffmpeg calls), so it's fully unit-tested below.
//! Turning a plan into actual ffmpeg arguments happens later in the encoder, because that depends
//! on which hardware encoder the machine has.

use serde::Serialize;

use crate::presets::{self, Budget, Preset};
use crate::probe::VideoInfo;

/// Fraction of the size cap we actually aim for: leaves room for the MP4 container and bitrate overshoot.
const SAFETY: f64 = 0.96;
/// Fixed container overhead (MP4 headers, index) in bytes.
const OVERHEAD_BYTES: f64 = 64_000.0;
/// Below this, H.264 video turns to mush even at 360p.
const MIN_VIDEO_KBPS: f64 = 150.0;
const MIN_AUDIO_KBPS: f64 = 48.0;
/// Bits per pixel per frame below which we'd rather lower fps/resolution than keep a blocky image.
const MIN_BITS_PER_PIXEL: f64 = 0.07;
/// Short-edge sizes we step down through when the bitrate is too low.
const RESOLUTION_LADDER: [u32; 5] = [1080, 720, 540, 480, 360];

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Already fits and is compatible: just repackage the streams, zero quality loss.
    Copy,
    Encode,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodePlan {
    pub mode: Mode,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// `Some(fps)` only when we change the frame rate; `None` keeps the source's timing untouched.
    pub target_fps: Option<f64>,
    pub video_kbps: u32,
    pub audio_kbps: u32,
    pub estimated_bytes: u64,
    /// Things the user should know before sending the file (e.g. X's 2:20 limit for free accounts).
    pub warnings: Vec<String>,
}

/// Rounds to the nearest even number (H.264 needs even dimensions), minimum 2.
fn even(x: f64) -> u32 {
    (((x / 2.0).round() * 2.0) as u32).max(2)
}

/// Scales so the short edge is at most `short`, keeping aspect ratio. Never upscales.
fn fit_short_edge(width: u32, height: u32, short: u32) -> (u32, u32) {
    let current_short = width.min(height);
    if current_short <= short {
        return (even(width as f64), even(height as f64));
    }
    let ratio = short as f64 / current_short as f64;
    (even(width as f64 * ratio), even(height as f64 * ratio))
}

fn bits_per_pixel(video_kbps: f64, width: u32, height: u32, fps: f64) -> f64 {
    video_kbps * 1000.0 / (width as f64 * height as f64 * fps)
}

fn format_duration(seconds: f64) -> String {
    let s = seconds.floor() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// The audio bitrate we'll use: preset default, never above the source, squeezed when the budget is tight.
fn pick_audio_kbps(info: &VideoInfo, preset: &Preset, total_kbps: f64) -> f64 {
    if info.audio_codec.is_none() {
        return 0.0;
    }
    let mut audio = preset.audio_kbps as f64;
    if let Some(source) = info.audio_bitrate_kbps {
        audio = audio.min(source as f64).max(MIN_AUDIO_KBPS);
    }
    let tight_cap = (total_kbps * 0.15).max(MIN_AUDIO_KBPS);
    audio.min(tight_cap)
}

fn is_compatible_as_is(info: &VideoInfo, preset: &Preset, source_fps: f64) -> bool {
    info.video_codec == "h264"
        && matches!(info.audio_codec.as_deref(), None | Some("aac"))
        && info.width.min(info.height) <= preset.max_short_edge
        && source_fps <= preset.max_fps + 0.5
}

pub fn plan(info: &VideoInfo, preset: &Preset) -> Result<EncodePlan, String> {
    // Unknown frame rate (rare, broken files): assume 30 so the math still works.
    let source_fps = if info.fps > 0.0 { info.fps } else { 30.0 };
    let mut warnings = Vec::new();

    if let Some(limit) = &preset.duration_warning {
        if info.duration_s > limit.max_s {
            warnings.push(format!("{} (this one is {})", limit.message, format_duration(info.duration_s)));
        }
    }

    let (mut width, mut height) = fit_short_edge(info.width, info.height, preset.max_short_edge);
    let mut fps = source_fps.min(preset.max_fps);

    let (video_kbps, audio_kbps) = match preset.budget {
        Budget::MaxBytes(cap) => {
            if info.size_bytes <= cap && is_compatible_as_is(info, preset, source_fps) {
                return Ok(copy_plan(info, source_fps, warnings));
            }

            let usable_bytes = cap as f64 * SAFETY - OVERHEAD_BYTES;
            let total_kbps = usable_bytes * 8.0 / 1000.0 / info.duration_s;
            let audio = pick_audio_kbps(info, preset, total_kbps);
            let video = total_kbps - audio;

            if video < MIN_VIDEO_KBPS {
                let min_audio = if info.audio_codec.is_some() { MIN_AUDIO_KBPS } else { 0.0 };
                let max_duration = usable_bytes * 8.0 / 1000.0 / (MIN_VIDEO_KBPS + min_audio);
                return Err(format!(
                    "Too long to fit in {}: the maximum is about {} for this preset",
                    presets::format_mb(cap),
                    format_duration(max_duration)
                ));
            }

            // Too few bits for this many pixels? Halve the frame rate first (motion stays smooth enough
            // at 30 fps), then step the resolution down the ladder.
            // (The UI explains any lowered resolution/fps itself by comparing the plan with the source.)
            while bits_per_pixel(video, width, height, fps) < MIN_BITS_PER_PIXEL {
                if fps > 31.0 {
                    fps = if fps >= 48.0 { fps / 2.0 } else { 30.0 };
                } else if let Some(&next) = RESOLUTION_LADDER.iter().find(|&&s| s < width.min(height)) {
                    (width, height) = fit_short_edge(width, height, next);
                } else {
                    break; // already at the smallest size; accept a softer picture
                }
            }
            (video, audio)
        }
        Budget::Quality { kbps_1080p30 } => {
            let pixel_rate = width as f64 * height as f64 * fps;
            let target = kbps_1080p30 as f64 * pixel_rate / (1920.0 * 1080.0 * 30.0);
            let audio = pick_audio_kbps(info, preset, f64::INFINITY);
            // Re-encoding can't add detail, so never spend more than the source did.
            let source_video = info.bitrate_kbps as f64 - info.audio_bitrate_kbps.unwrap_or(0) as f64;
            let video = target.min(source_video.max(MIN_VIDEO_KBPS));

            if is_compatible_as_is(info, preset, source_fps) && source_video <= target {
                return Ok(copy_plan(info, source_fps, warnings));
            }
            (video, audio)
        }
    };

    let fps_lowered = fps < source_fps - 0.5;

    Ok(EncodePlan {
        mode: Mode::Encode,
        width,
        height,
        fps,
        target_fps: fps_lowered.then_some(fps),
        video_kbps: video_kbps.round() as u32,
        audio_kbps: audio_kbps.round() as u32,
        estimated_bytes: ((video_kbps + audio_kbps) * 1000.0 / 8.0 * info.duration_s + OVERHEAD_BYTES) as u64,
        warnings,
    })
}

fn copy_plan(info: &VideoInfo, fps: f64, warnings: Vec<String>) -> EncodePlan {
    EncodePlan {
        mode: Mode::Copy,
        width: info.width,
        height: info.height,
        fps,
        target_fps: None,
        video_kbps: info.bitrate_kbps.saturating_sub(info.audio_bitrate_kbps.unwrap_or(0)),
        audio_kbps: info.audio_bitrate_kbps.unwrap_or(0),
        estimated_bytes: info.size_bytes,
        warnings,
    }
}

/// Called from the UI with `invoke("plan_compression", { info, presetId })`.
#[tauri::command]
pub fn plan_compression(info: VideoInfo, preset_id: String) -> Result<EncodePlan, String> {
    let preset = presets::find(&preset_id).ok_or_else(|| format!("Unknown preset '{preset_id}'"))?;
    plan(&info, preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a test video. Bitrate is derived from size and duration like a real file.
    fn video(width: u32, height: u32, fps: f64, duration_s: f64, size_bytes: u64) -> VideoInfo {
        VideoInfo {
            path: "test.mp4".into(),
            size_bytes,
            duration_s,
            width,
            height,
            fps,
            video_codec: "h264".into(),
            bitrate_kbps: (size_bytes as f64 * 8.0 / 1000.0 / duration_s) as u32,
            audio_codec: Some("aac".into()),
            audio_bitrate_kbps: Some(192),
        }
    }

    fn preset(id: &str) -> &'static Preset {
        presets::find(id).unwrap()
    }

    const DISCORD_CAP: u64 = 20_000_000;

    #[test]
    fn long_1080p60_drops_fps_then_resolution_to_fit_discord() {
        let p = plan(&video(1920, 1080, 60.0, 60.0, 150_000_000), preset("discord-free")).unwrap();
        assert_eq!(p.mode, Mode::Encode);
        assert_eq!((p.width, p.height), (1280, 720));
        assert_eq!(p.fps, 30.0);
        assert!(p.estimated_bytes <= DISCORD_CAP, "estimated {} bytes", p.estimated_bytes);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    }

    #[test]
    fn small_compatible_clip_is_copied() {
        let p = plan(&video(1280, 720, 30.0, 10.0, 3_000_000), preset("discord-free")).unwrap();
        assert_eq!(p.mode, Mode::Copy);
        assert_eq!(p.estimated_bytes, 3_000_000);
    }

    #[test]
    fn small_but_incompatible_clip_is_reencoded() {
        let mut v = video(1280, 720, 30.0, 10.0, 3_000_000);
        v.video_codec = "hevc".into(); // Discord can't preview HEVC in every client
        assert_eq!(plan(&v, preset("discord-free")).unwrap().mode, Mode::Encode);
    }

    #[test]
    fn far_too_long_video_explains_max_duration() {
        let err = plan(&video(1920, 1080, 30.0, 1800.0, 900_000_000), preset("discord-free")).unwrap_err();
        assert!(err.contains("20 MB"), "{err}");
        assert!(err.contains("12:53"), "{err}"); // 19.136 MB / (150 + 48 kbps)
    }

    #[test]
    fn portrait_video_stays_portrait_with_even_dimensions() {
        let p = plan(&video(1080, 1920, 30.0, 60.0, 150_000_000), preset("discord-free")).unwrap();
        assert!(p.height > p.width);
        assert_eq!((p.width % 2, p.height % 2), (0, 0));
        assert_eq!((p.width, p.height), (720, 1280));
    }

    #[test]
    fn video_without_audio_spends_everything_on_video() {
        let mut v = video(1920, 1080, 30.0, 30.0, 80_000_000);
        v.audio_codec = None;
        v.audio_bitrate_kbps = None;
        let p = plan(&v, preset("discord-free")).unwrap();
        assert_eq!(p.audio_kbps, 0);
        assert!(p.estimated_bytes <= DISCORD_CAP);
    }

    #[test]
    fn x_warns_about_free_account_length_limit() {
        let p = plan(&video(1920, 1080, 30.0, 180.0, 300_000_000), preset("x")).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("2:20")), "{:?}", p.warnings);
    }

    #[test]
    fn instagram_caps_frame_rate_and_resolution() {
        let p = plan(&video(3840, 2160, 60.0, 30.0, 400_000_000), preset("instagram")).unwrap();
        assert_eq!((p.width, p.height), (1920, 1080));
        assert_eq!(p.fps, 30.0);
        assert_eq!(p.video_kbps, 5_000);
    }

    #[test]
    fn youtube_never_exceeds_source_bitrate() {
        // 4K30 at ~10 Mbps: YouTube's 4K target (~48 Mbps) would be wasteful.
        let v = video(3840, 2160, 30.0, 60.0, 75_000_000);
        let p = plan(&v, preset("youtube")).unwrap();
        assert!(p.video_kbps <= v.bitrate_kbps, "{} > {}", p.video_kbps, v.bitrate_kbps);
        assert_eq!((p.width, p.height), (3840, 2160));
    }

    #[test]
    fn short_clip_keeps_full_quality_settings() {
        // 10 s of 1080p60 has plenty of budget in 20 MB: keep resolution and frame rate.
        let p = plan(&video(1920, 1080, 60.0, 10.0, 40_000_000), preset("discord-free")).unwrap();
        assert_eq!((p.width, p.height, p.fps), (1920, 1080, 60.0));
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    }
}
