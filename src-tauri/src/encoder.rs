//! Turns an `EncodePlan` into an ffmpeg command line, and reads ffmpeg's progress output.
//! All pure functions, so they are unit-tested below; actually running ffmpeg lives in `jobs.rs`.

use std::path::{Path, PathBuf};

use crate::planner::{EncodePlan, Mode};

/// The H.264 encoders we can use, in order of preference.
/// `#[derive(Clone, Copy)]` lets us pass it around by value like a number (it holds no data).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoEncoder {
    Nvenc,
    Qsv,
    Amf,
    /// Windows Media Foundation: ships with every Windows 10/11, LGPL-safe, Windows covers H.264
    /// patent licensing. Always available, so it's the fallback.
    MediaFoundation,
}

impl VideoEncoder {
    /// Hardware encoders worth probing, best first. `MediaFoundation` is the fallback, not probed.
    pub const HARDWARE: [VideoEncoder; 3] = [VideoEncoder::Nvenc, VideoEncoder::Qsv, VideoEncoder::Amf];

    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            VideoEncoder::Nvenc => "h264_nvenc",
            VideoEncoder::Qsv => "h264_qsv",
            VideoEncoder::Amf => "h264_amf",
            VideoEncoder::MediaFoundation => "h264_mf",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VideoEncoder::Nvenc => "NVIDIA NVENC",
            VideoEncoder::Qsv => "Intel Quick Sync",
            VideoEncoder::Amf => "AMD AMF",
            VideoEncoder::MediaFoundation => "Windows (software)",
        }
    }

    /// We ask each encoder for slightly less than the budget to absorb its typical overshoot.
    /// Measured on a noisy 20 s clip at 300k/1200k/2400k (RTX 3060 Ti, Oct 2026):
    /// NVENC CBR + multipass: +0.3..+1.1%; Media Foundation CBR: ~+2%.
    /// QSV/AMF are untested (no hardware here), so they get a conservative 5%.
    fn bitrate_scale(self) -> f64 {
        match self {
            VideoEncoder::Nvenc => 0.99,
            VideoEncoder::MediaFoundation => 0.98,
            VideoEncoder::Qsv | VideoEncoder::Amf => 0.95,
        }
    }

    /// Encoder-specific flags for a target video bitrate.
    fn args(self, video_kbps: u32) -> Vec<String> {
        let kbps = (video_kbps as f64 * self.bitrate_scale()).round() as u32;
        let rate = format!("{kbps}k");
        let buffer = format!("{}k", kbps * 2);
        let flags: Vec<&str> = match self {
            // CBR + full-res multipass hit size targets within ~1%; VBR with a looser maxrate overshot 4-9%.
            VideoEncoder::Nvenc => vec!["-preset", "p5", "-tune", "hq", "-rc", "cbr", "-multipass", "fullres",
                "-spatial-aq", "1", "-pix_fmt", "yuv420p", "-b:v", &rate, "-bufsize", &buffer],
            // UNTESTED: maxrate = bitrate makes QSV use CBR, per FFmpeg's QSV docs.
            VideoEncoder::Qsv => vec!["-preset", "slow", "-pix_fmt", "nv12", "-b:v", &rate, "-maxrate", &rate,
                "-bufsize", &buffer],
            // UNTESTED: AMF's constant-bitrate mode.
            VideoEncoder::Amf => vec!["-quality", "quality", "-rc", "cbr", "-pix_fmt", "nv12", "-b:v", &rate,
                "-bufsize", &buffer],
            // h264_mf ignored `-rate_control` in tests (all modes gave identical files); cbr kept for clarity.
            VideoEncoder::MediaFoundation => vec!["-rate_control", "cbr", "-scenario", "archive", "-pix_fmt", "nv12",
                "-b:v", &rate],
        };
        let mut args = vec!["-c:v".to_string(), self.ffmpeg_name().to_string()];
        args.extend(flags.into_iter().map(String::from));
        args
    }
}

/// ffmpeg arguments that try a 1-frame encode: succeeds only if the GPU + driver support this encoder.
pub fn probe_encoder_args(encoder: VideoEncoder) -> Vec<String> {
    ["-hide_banner", "-v", "error", "-f", "lavfi", "-i", "color=black:s=256x256:d=0.1", "-frames:v", "1"]
        .into_iter()
        .chain(["-c:v", encoder.ffmpeg_name(), "-f", "null", "-"])
        .map(String::from)
        .collect()
}

/// `clip.mov` → `clip_discord.mp4`, in `output_dir` if given, else next to the original. If that's taken
/// (or would be the input itself), tries `clip_discord (2).mp4`, `(3)`, ... We never overwrite anything.
///
/// `exists` is passed in (instead of calling the file system directly) so tests can fake it.
/// `impl Fn(&Path) -> bool` means "any function/closure taking a path and returning a bool".
pub fn output_path(
    input: &Path,
    output_dir: Option<&Path>,
    suffix: &str,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    let dir = output_dir.unwrap_or_else(|| input.parent().unwrap_or(Path::new("")));
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("video");

    let mut candidate = dir.join(format!("{stem}_{suffix}.mp4"));
    let mut n = 2;
    while candidate == input || exists(&candidate) {
        candidate = dir.join(format!("{stem}_{suffix} ({n}).mp4"));
        n += 1;
    }
    candidate
}

pub fn build_args(input: &str, output: &str, plan: &EncodePlan, encoder: VideoEncoder) -> Vec<String> {
    // `-v error` keeps stderr down to real problems, which we show to the user if ffmpeg fails.
    let mut args: Vec<String> = ["-hide_banner", "-v", "error", "-nostdin", "-y", "-i", input, "-map", "0:v:0"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    match plan.mode {
        Mode::Copy => {
            // The trailing `?` means "only if the input has an audio stream".
            args.extend(["-map", "0:a:0?", "-c", "copy"].map(String::from));
        }
        Mode::Encode => {
            let mut filters = format!("scale={}:{}", plan.width, plan.height);
            if let Some(fps) = plan.target_fps {
                filters.push_str(&format!(",fps={fps}"));
            }
            args.extend(["-vf".into(), filters]);
            args.extend(encoder.args(plan.video_kbps));
            if plan.audio_kbps > 0 {
                args.extend(["-map".into(), "0:a:0".into(), "-c:a".into(), "aac".into()]);
                args.extend(["-b:a".into(), format!("{}k", plan.audio_kbps), "-ac".into(), "2".into()]);
            } else {
                args.push("-an".into());
            }
        }
    }

    // `+faststart` moves the MP4 index to the front so Discord/browsers can start playing immediately.
    // `-progress pipe:1` makes ffmpeg print machine-readable `key=value` progress lines on stdout.
    args.extend(["-movflags", "+faststart", "-progress", "pipe:1", "-nostats", output].map(String::from));
    args
}

/// One meaningful line of ffmpeg's `-progress` output.
#[derive(Debug, PartialEq)]
pub enum ProgressLine {
    /// Position in the output, in microseconds.
    OutTimeUs(i64),
    /// Encoding speed relative to real time (2.5 = 2.5× faster than playback).
    Speed(f64),
    /// ffmpeg finished a progress block; `true` on the very last one.
    BlockEnd { finished: bool },
}

pub fn parse_progress_line(line: &str) -> Option<ProgressLine> {
    let (key, value) = line.trim().split_once('=')?;
    match key {
        "out_time_us" => value.parse().ok().map(ProgressLine::OutTimeUs),
        "speed" => value.trim_end_matches('x').trim().parse().ok().map(ProgressLine::Speed),
        "progress" => Some(ProgressLine::BlockEnd { finished: value == "end" }),
        _ => None, // `_` matches anything else; ffmpeg prints many keys we don't need
    }
}

pub fn percent(out_time_us: i64, duration_s: f64) -> f64 {
    (out_time_us as f64 / 1_000_000.0 / duration_s * 100.0).clamp(0.0, 100.0)
}

/// Video bitrate for the next attempt after an output came out too big.
/// Scales by how far off we were (estimated / actual) with an extra 3% safety margin.
/// Only the video share is reduced; the audio bitrate stays as planned.
/// `estimated_bytes` stays the original target across retries, so calling this again after a second
/// attempt (with the lowered `video_kbps`) corrects from where that attempt landed.
pub fn retry_video_kbps(plan: &EncodePlan, actual_bytes: u64) -> u32 {
    let ratio = plan.estimated_bytes as f64 / actual_bytes as f64;
    (plan.video_kbps as f64 * ratio * 0.97).floor() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_plan(audio_kbps: u32, target_fps: Option<f64>) -> EncodePlan {
        EncodePlan {
            mode: Mode::Encode,
            width: 1280,
            height: 720,
            fps: target_fps.unwrap_or(60.0),
            target_fps,
            video_kbps: 2400,
            audio_kbps,
            estimated_bytes: 0,
            warnings: vec![],
        }
    }

    /// True if `args` contains `flag` immediately followed by `value`.
    fn has_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn output_name_gets_suffix_and_mp4_extension() {
        let out = output_path(Path::new(r"C:\videos\clip.mov"), None, "discord", |_| false);
        assert_eq!(out, PathBuf::from(r"C:\videos\clip_discord.mp4"));
    }

    #[test]
    fn output_goes_to_the_chosen_folder() {
        let out = output_path(Path::new(r"C:\videos\clip.mov"), Some(Path::new(r"D:\out")), "x", |_| false);
        assert_eq!(out, PathBuf::from(r"D:\out\clip_x.mp4"));
    }

    #[test]
    fn output_name_avoids_existing_files() {
        let taken = [PathBuf::from(r"C:\v\clip_discord.mp4"), PathBuf::from(r"C:\v\clip_discord (2).mp4")];
        let out = output_path(Path::new(r"C:\v\clip.mp4"), None, "discord", |p| taken.iter().any(|t| t == p));
        assert_eq!(out, PathBuf::from(r"C:\v\clip_discord (3).mp4"));
    }

    #[test]
    fn output_never_overwrites_the_input() {
        let input = Path::new(r"C:\v\clip_discord.mp4");
        // stem is "clip_discord" so the first candidate is clip_discord_discord.mp4: different from input
        assert_ne!(output_path(input, None, "discord", |_| false), input);
    }

    const MF: VideoEncoder = VideoEncoder::MediaFoundation;

    #[test]
    fn encode_args_contain_scale_bitrates_and_audio() {
        let args = build_args("in.mov", "out.mp4", &encode_plan(128, None), MF);
        assert!(has_pair(&args, "-vf", "scale=1280:720"));
        assert!(has_pair(&args, "-c:v", "h264_mf"));
        assert!(has_pair(&args, "-b:v", "2352k")); // 2400 × 0.98 overshoot allowance
        assert!(has_pair(&args, "-b:a", "128k"));
        assert!(has_pair(&args, "-i", "in.mov"));
        assert_eq!(args.last().unwrap(), "out.mp4");
    }

    #[test]
    fn nvenc_uses_cbr_multipass_with_scaled_bitrate() {
        let args = build_args("in.mp4", "out.mp4", &encode_plan(128, None), VideoEncoder::Nvenc);
        assert!(has_pair(&args, "-c:v", "h264_nvenc"));
        assert!(has_pair(&args, "-rc", "cbr"));
        assert!(has_pair(&args, "-multipass", "fullres"));
        assert!(has_pair(&args, "-b:v", "2376k")); // 2400 × 0.99
    }

    #[test]
    fn every_encoder_sets_its_codec_and_a_bitrate() {
        for encoder in VideoEncoder::HARDWARE.into_iter().chain([MF]) {
            let args = build_args("in.mp4", "out.mp4", &encode_plan(128, None), encoder);
            assert!(has_pair(&args, "-c:v", encoder.ffmpeg_name()), "{encoder:?}");
            assert!(args.contains(&"-b:v".to_string()), "{encoder:?}");
        }
    }

    #[test]
    fn probe_args_try_one_frame_with_the_encoder() {
        let args = probe_encoder_args(VideoEncoder::Qsv);
        assert!(has_pair(&args, "-c:v", "h264_qsv"));
        assert!(has_pair(&args, "-frames:v", "1"));
        assert!(has_pair(&args, "-f", "null"));
    }

    #[test]
    fn fps_filter_only_when_frame_rate_is_lowered() {
        let args = build_args("in.mp4", "out.mp4", &encode_plan(128, Some(30.0)), MF);
        assert!(has_pair(&args, "-vf", "scale=1280:720,fps=30"));
    }

    #[test]
    fn no_audio_means_an_flag() {
        let args = build_args("in.mp4", "out.mp4", &encode_plan(0, None), MF);
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"-b:a".to_string()));
    }

    #[test]
    fn copy_mode_does_not_reencode() {
        let mut plan = encode_plan(128, None);
        plan.mode = Mode::Copy;
        let args = build_args("in.mp4", "out.mp4", &plan, MF);
        assert!(has_pair(&args, "-c", "copy"));
        assert!(!args.contains(&"-vf".to_string()));
    }

    #[test]
    fn retry_lowers_video_bitrate_proportionally() {
        let mut plan = encode_plan(128, None);
        plan.estimated_bytes = 19_200_000;
        // 10% over → 2400 × (19.2 / 21.12) × 0.97 = 2116
        assert_eq!(retry_video_kbps(&plan, 21_120_000), 2116);
        // A second retry starts from the already-lowered bitrate.
        plan.video_kbps = 2116;
        assert!(retry_video_kbps(&plan, 19_500_000) < 2116);
    }

    #[test]
    fn parses_progress_lines() {
        assert_eq!(parse_progress_line("out_time_us=1500000"), Some(ProgressLine::OutTimeUs(1_500_000)));
        assert_eq!(parse_progress_line("speed=3.25x"), Some(ProgressLine::Speed(3.25)));
        assert_eq!(parse_progress_line("speed=N/A"), None);
        assert_eq!(parse_progress_line("progress=end"), Some(ProgressLine::BlockEnd { finished: true }));
        assert_eq!(parse_progress_line("frame=42"), None);
    }

    #[test]
    fn percent_is_clamped() {
        assert_eq!(percent(5_000_000, 10.0), 50.0);
        assert_eq!(percent(-1, 10.0), 0.0);
        assert_eq!(percent(11_000_000, 10.0), 100.0);
    }

    /// Real end-to-end encode with the sidecar ffmpeg. Slow, so opt-in: `cargo test -- --ignored`.
    /// Uses the best hardware encoder this machine has (falls back to Media Foundation).
    #[test]
    #[ignore]
    fn real_encode_fits_discord_free_best_encoder() {
        let ffmpeg = sidecar("ffmpeg");
        let best = VideoEncoder::HARDWARE
            .into_iter()
            .find(|&e| std::process::Command::new(&ffmpeg).args(probe_encoder_args(e)).status().unwrap().success())
            .unwrap_or(MF);
        real_encode_fits_discord_free(best);
    }

    #[test]
    #[ignore]
    fn real_encode_fits_discord_free_media_foundation() {
        real_encode_fits_discord_free(MF);
    }

    fn sidecar(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries").join(format!("{name}-x86_64-pc-windows-msvc.exe"))
    }

    fn real_encode_fits_discord_free(encoder: VideoEncoder) {
        use std::process::Command;

        let ffmpeg = sidecar("ffmpeg");
        let ffprobe = sidecar("ffprobe");
        let dir = std::env::temp_dir().join(format!("bonsai-it-{}-{}", std::process::id(), encoder.ffmpeg_name()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mp4");

        // 60 s of noisy 1080p60 + a tone: forces the planner to drop to 720p30.
        let status = Command::new(&ffmpeg)
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=60,noise=alls=12:allf=t+u"])
            // h264_mf: the bundled FFmpeg only has hardware/Windows H.264 encoders (no libopenh264/libx264).
            .args(["-f", "lavfi", "-i", "sine=frequency=440", "-t", "60", "-c:v", "h264_mf", "-pix_fmt", "nv12"])
            .args(["-b:v", "12M"])
            .args(["-c:a", "aac", src.to_str().unwrap()])
            .status()
            .unwrap();
        assert!(status.success());

        let probe = Command::new(&ffprobe)
            .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams", src.to_str().unwrap()])
            .output()
            .unwrap();
        let size = std::fs::metadata(&src).unwrap().len();
        let info = crate::probe::parse_ffprobe_json(src.to_str().unwrap(), &String::from_utf8_lossy(&probe.stdout), size)
            .unwrap();
        let plan = crate::planner::plan(&info, crate::presets::find("discord-free").unwrap()).unwrap();

        let out = dir.join("out.mp4");
        let status = Command::new(&ffmpeg)
            .args(build_args(src.to_str().unwrap(), out.to_str().unwrap(), &plan, encoder))
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        let out_size = std::fs::metadata(&out).unwrap().len();
        println!(
            "{}: plan {}x{} @ {} kbps, estimated {} B, actual {} B",
            encoder.label(), plan.width, plan.height, plan.video_kbps, plan.estimated_bytes, out_size
        );
        std::fs::remove_dir_all(&dir).ok();
        assert!(out_size <= 20_000_000, "output {out_size} bytes exceeds 20 MB");
    }
}
