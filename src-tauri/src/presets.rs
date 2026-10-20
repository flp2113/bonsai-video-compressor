//! The destination presets. Changing a platform limit should only ever mean editing this table.

use serde::Serialize;

/// How a preset decides how many bits to spend.
///
/// Rust enums can carry data in each variant (like a TS discriminated union):
/// `Budget::MaxBytes(20_000_000)` vs `Budget::Quality { kbps_1080p30: 5_000 }`.
pub enum Budget {
    /// Output must not exceed this many bytes (Discord, X).
    MaxBytes(u64),
    /// No size cap: aim for this video bitrate at 1080p30, scaled for other resolutions/frame rates.
    Quality { kbps_1080p30: u32 },
}

/// A soft limit we only warn about (we still compress the video).
pub struct DurationWarning {
    pub max_s: f64,
    pub message: &'static str,
}

pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    /// Appended to output file names: `clip.mov` → `clip_discord.mp4`.
    pub suffix: &'static str,
    pub budget: Budget,
    /// Limit on the *short* edge (720 = "720p"), so portrait and landscape videos are treated alike.
    pub max_short_edge: u32,
    pub max_fps: f64,
    pub audio_kbps: u32,
    pub duration_warning: Option<DurationWarning>,
}

/// 1 MB = 1,000,000 bytes: the stricter reading of "MB", so we never overshoot a limit.
const MB: u64 = 1_000_000;

/// `&[Preset]` is a slice: a read-only view of a fixed list, baked into the binary at compile time.
pub const PRESETS: &[Preset] = &[
    Preset {
        id: "discord-free",
        label: "Discord Free",
        suffix: "discord",
        budget: Budget::MaxBytes(20 * MB),
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 128,
        duration_warning: None,
    },
    Preset {
        id: "discord-nitro-basic",
        label: "Discord Nitro Basic",
        suffix: "discord-basic",
        budget: Budget::MaxBytes(50 * MB),
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 160,
        duration_warning: None,
    },
    Preset {
        id: "discord-nitro",
        label: "Discord Nitro",
        suffix: "discord-nitro",
        budget: Budget::MaxBytes(500 * MB),
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 192,
        duration_warning: None,
    },
    Preset {
        id: "x",
        label: "X / Twitter",
        suffix: "x",
        budget: Budget::MaxBytes(512 * MB),
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 128,
        duration_warning: Some(DurationWarning {
            max_s: 140.0,
            message: "Free X accounts allow videos up to 2:20",
        }),
    },
    Preset {
        id: "instagram",
        label: "Instagram",
        suffix: "instagram",
        budget: Budget::Quality { kbps_1080p30: 5_000 },
        max_short_edge: 1080,
        max_fps: 30.0,
        audio_kbps: 128,
        duration_warning: None,
    },
    Preset {
        id: "youtube",
        label: "YouTube",
        suffix: "youtube",
        budget: Budget::Quality { kbps_1080p30: 12_000 },
        max_short_edge: 2160,
        max_fps: 60.0,
        audio_kbps: 192,
        duration_warning: None,
    },
];

pub fn find(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Human-readable size, e.g. `20 MB`.
pub fn format_mb(bytes: u64) -> String {
    format!("{} MB", bytes / MB)
}

/// What the UI needs to draw a preset button.
#[derive(Serialize)]
pub struct PresetSummary {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: String,
}

#[tauri::command]
pub fn list_presets() -> Vec<PresetSummary> {
    PRESETS
        .iter()
        .map(|p| PresetSummary {
            id: p.id,
            label: p.label,
            // `match` is like a `switch` that the compiler checks covers every variant.
            hint: match p.budget {
                Budget::MaxBytes(bytes) => format!("≤ {}", format_mb(bytes)),
                Budget::Quality { .. } => "Best quality".to_string(),
            },
        })
        .collect()
}
