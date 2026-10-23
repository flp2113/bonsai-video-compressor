//! The destination presets. Changing a platform limit should only ever mean editing this table.

use serde::Serialize;

/// A soft limit we only warn about (we still compress the video).
pub struct DurationWarning {
    pub max_s: f64,
    pub message: &'static str,
}

pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    /// Destination shown as one row in the UI ("Discord"); presets of the same group sit next to each other.
    pub group: &'static str,
    /// Short name within a group with several options ("Free", "Basic", "Nitro"); `None` for single-preset groups.
    pub tier: Option<&'static str>,
    /// Appended to output file names: `clip.mov` → `clip_discord.mp4`.
    pub suffix: &'static str,
    /// Output must not exceed this many bytes.
    pub max_bytes: u64,
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
        group: "Discord",
        tier: Some("Free"),
        suffix: "discord",
        max_bytes: 20 * MB,
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 128,
        duration_warning: None,
    },
    Preset {
        id: "discord-nitro-basic",
        label: "Discord Nitro Basic",
        group: "Discord",
        tier: Some("Basic"),
        suffix: "discord-basic",
        max_bytes: 50 * MB,
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 160,
        duration_warning: None,
    },
    Preset {
        id: "discord-nitro",
        label: "Discord Nitro",
        group: "Discord",
        tier: Some("Nitro"),
        suffix: "discord-nitro",
        max_bytes: 500 * MB,
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 192,
        duration_warning: None,
    },
    Preset {
        id: "x",
        label: "X / Twitter",
        group: "X",
        tier: None,
        suffix: "x",
        max_bytes: 512 * MB,
        max_short_edge: 1080,
        max_fps: 60.0,
        audio_kbps: 128,
        duration_warning: Some(DurationWarning {
            max_s: 140.0,
            message: "Free X accounts allow videos up to 2:20",
        }),
    },
    // WhatsApp Help Center ("How to send media"): 100 MB at 720p on a fast connection.
    // 30 fps is our choice, not a documented limit: phones record at 30 and it leaves more bits per frame.
    Preset {
        id: "whatsapp",
        label: "WhatsApp",
        group: "WhatsApp",
        tier: None,
        suffix: "whatsapp",
        max_bytes: 100 * MB,
        max_short_edge: 720,
        max_fps: 30.0,
        audio_kbps: 128,
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

/// What the UI needs to draw a destination in the list.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetSummary {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub tier: Option<&'static str>,
    /// The size limit, so the UI can mark it on the size meter.
    pub cap_bytes: u64,
    /// Shown next to the destination, e.g. "20 MB".
    pub hint: String,
}

#[tauri::command]
pub fn list_presets() -> Vec<PresetSummary> {
    PRESETS
        .iter()
        .map(|p| PresetSummary {
            id: p.id,
            label: p.label,
            group: p.group,
            tier: p.tier,
            cap_bytes: p.max_bytes,
            hint: format_mb(p.max_bytes),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_are_contiguous_and_multi_preset_groups_have_tiers() {
        let mut seen: Vec<&str> = Vec::new();
        for (i, p) in PRESETS.iter().enumerate() {
            let continues_group = i > 0 && PRESETS[i - 1].group == p.group;
            if !continues_group {
                assert!(!seen.contains(&p.group), "group '{}' is split up in PRESETS", p.group);
                seen.push(p.group);
            }
            let group_size = PRESETS.iter().filter(|q| q.group == p.group).count();
            assert_eq!(p.tier.is_some(), group_size > 1, "preset '{}' tier", p.id);
        }
    }
}
