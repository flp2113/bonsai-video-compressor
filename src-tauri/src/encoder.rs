//! Turns an `EncodePlan` into an ffmpeg command line, and reads ffmpeg's progress output.
//! All pure functions, so they are unit-tested below; actually running ffmpeg lives in `jobs.rs`.

use std::path::{Path, PathBuf};

use crate::planner::{EncodePlan, Mode};

/// Video encoder for now: Windows Media Foundation's H.264 encoder. It ships with every Windows 10/11,
/// is LGPL-safe, and Windows covers H.264 patent licensing. Measured ~2% bitrate overshoot at the
/// resolutions the planner picks (well inside its 4% safety margin).
/// Milestone 5 adds NVIDIA/Intel/AMD hardware encoders in front of it.
const VIDEO_ENCODER: &str = "h264_mf";

/// `clip.mov` → `clip_discord.mp4` in the same folder. If that's taken (or would be the input itself),
/// tries `clip_discord (2).mp4`, `(3)`, ... We never overwrite anything.
///
/// `exists` is passed in (instead of calling the file system directly) so tests can fake it.
/// `impl Fn(&Path) -> bool` means "any function/closure taking a path and returning a bool".
pub fn output_path(input: &Path, suffix: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new(""));
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("video");

    let mut candidate = dir.join(format!("{stem}_{suffix}.mp4"));
    let mut n = 2;
    while candidate == input || exists(&candidate) {
        candidate = dir.join(format!("{stem}_{suffix} ({n}).mp4"));
        n += 1;
    }
    candidate
}

pub fn build_args(input: &str, output: &str, plan: &EncodePlan) -> Vec<String> {
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
            args.extend([
                "-vf".into(),
                filters,
                "-c:v".into(),
                VIDEO_ENCODER.into(),
                "-rate_control".into(),
                "cbr".into(),
                "-scenario".into(),
                "archive".into(),
                "-pix_fmt".into(),
                "nv12".into(),
                "-b:v".into(),
                format!("{}k", plan.video_kbps),
            ]);
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
        let out = output_path(Path::new(r"C:\videos\clip.mov"), "discord", |_| false);
        assert_eq!(out, PathBuf::from(r"C:\videos\clip_discord.mp4"));
    }

    #[test]
    fn output_name_avoids_existing_files() {
        let taken = [PathBuf::from(r"C:\v\clip_discord.mp4"), PathBuf::from(r"C:\v\clip_discord (2).mp4")];
        let out = output_path(Path::new(r"C:\v\clip.mp4"), "discord", |p| taken.iter().any(|t| t == p));
        assert_eq!(out, PathBuf::from(r"C:\v\clip_discord (3).mp4"));
    }

    #[test]
    fn output_never_overwrites_the_input() {
        let input = Path::new(r"C:\v\clip_discord.mp4");
        // stem is "clip_discord" so the first candidate is clip_discord_discord.mp4: different from input
        assert_ne!(output_path(input, "discord", |_| false), input);
    }

    #[test]
    fn encode_args_contain_scale_bitrates_and_audio() {
        let args = build_args("in.mov", "out.mp4", &encode_plan(128, None));
        assert!(has_pair(&args, "-vf", "scale=1280:720"));
        assert!(has_pair(&args, "-c:v", "h264_mf"));
        assert!(has_pair(&args, "-b:v", "2400k"));
        assert!(has_pair(&args, "-b:a", "128k"));
        assert!(has_pair(&args, "-i", "in.mov"));
        assert_eq!(args.last().unwrap(), "out.mp4");
    }

    #[test]
    fn fps_filter_only_when_frame_rate_is_lowered() {
        let args = build_args("in.mp4", "out.mp4", &encode_plan(128, Some(30.0)));
        assert!(has_pair(&args, "-vf", "scale=1280:720,fps=30"));
    }

    #[test]
    fn no_audio_means_an_flag() {
        let args = build_args("in.mp4", "out.mp4", &encode_plan(0, None));
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"-b:a".to_string()));
    }

    #[test]
    fn copy_mode_does_not_reencode() {
        let mut plan = encode_plan(128, None);
        plan.mode = Mode::Copy;
        let args = build_args("in.mp4", "out.mp4", &plan);
        assert!(has_pair(&args, "-c", "copy"));
        assert!(!args.contains(&"-vf".to_string()));
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
    #[test]
    #[ignore]
    fn real_encode_fits_discord_free() {
        use std::process::Command;

        let bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        let ffmpeg = bin.join("ffmpeg-x86_64-pc-windows-msvc.exe");
        let ffprobe = bin.join("ffprobe-x86_64-pc-windows-msvc.exe");
        let dir = std::env::temp_dir().join(format!("bonsai-it-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.mp4");

        // 60 s of noisy 1080p60 + a tone: forces the planner to drop to 720p30.
        let status = Command::new(&ffmpeg)
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=60,noise=alls=12:allf=t+u"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440", "-t", "60", "-c:v", "libopenh264", "-b:v", "12M"])
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
            .args(build_args(src.to_str().unwrap(), out.to_str().unwrap(), &plan))
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());

        let out_size = std::fs::metadata(&out).unwrap().len();
        println!("plan {}x{} @ {} kbps, estimated {} B, actual {} B", plan.width, plan.height, plan.video_kbps, plan.estimated_bytes, out_size);
        std::fs::remove_dir_all(&dir).ok();
        assert!(out_size <= 20_000_000, "output {out_size} bytes exceeds 20 MB");
    }
}
