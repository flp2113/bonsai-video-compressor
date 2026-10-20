//! Runs ffmpeg in the background, reports progress to the UI, and supports cancelling.
//!
//! Flow: `compress` starts ffmpeg and immediately returns a job id. A background task then reads
//! ffmpeg's output and emits events the UI listens to: `job-progress`, `job-done`, `job-error`.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::encoder::{self, ProgressLine};
use crate::presets::{self, Budget};
use crate::probe::VideoInfo;
use crate::planner;

/// Shared app state for running jobs, registered once with `app.manage(Jobs::default())`.
///
/// Tauri may run commands on several threads at once, so shared data must be thread-safe:
/// - `AtomicU32` is a number that can be incremented safely from any thread.
/// - `Mutex<T>` lets only one thread at a time touch the `HashMap` inside (`.lock()` waits for its turn).
#[derive(Default)]
pub struct Jobs {
    next_id: AtomicU32,
    running: Mutex<HashMap<u32, CommandChild>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobProgress {
    job_id: u32,
    percent: f64,
    speed: Option<f64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobDone {
    job_id: u32,
    output_path: String,
    size_bytes: u64,
    /// Output ended up over the preset's size limit (milestone 5 will retry automatically).
    over_cap: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobError {
    job_id: u32,
    message: String,
    cancelled: bool,
}

/// Starts compressing and returns the job id right away; results arrive as events.
#[tauri::command]
pub async fn compress(
    app: AppHandle,
    jobs: State<'_, Jobs>,
    info: VideoInfo,
    preset_id: String,
) -> Result<u32, String> {
    let preset = presets::find(&preset_id).ok_or_else(|| format!("Unknown preset '{preset_id}'"))?;
    let plan = planner::plan(&info, preset)?;
    let output = encoder::output_path(Path::new(&info.path), preset.suffix, |p| p.exists());
    let output = output.to_string_lossy().into_owned();
    let cap = match preset.budget {
        Budget::MaxBytes(bytes) => Some(bytes),
        Budget::Quality { .. } => None,
    };

    let (rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| e.to_string())?
        .args(encoder::build_args(&info.path, &output, &plan))
        .spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {e}"))?;

    let job_id = jobs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    jobs.running.lock().unwrap().insert(job_id, child);

    // Hand the rest off to a background task so this command returns immediately.
    // `move` gives the task its own copies/ownership of the variables it uses.
    tauri::async_runtime::spawn(watch_job(app, rx, job_id, info.duration_s, output, cap));
    Ok(job_id)
}

/// Stops a running job. The background task notices ffmpeg died and cleans up.
#[tauri::command]
pub fn cancel(jobs: State<'_, Jobs>, job_id: u32) -> Result<(), String> {
    // Removing the child from the map is also how `watch_job` knows this was a cancel, not a crash.
    let child = jobs.running.lock().unwrap().remove(&job_id);
    match child {
        Some(child) => child.kill().map_err(|e| e.to_string()),
        None => Ok(()), // already finished
    }
}

async fn watch_job(
    app: AppHandle,
    mut rx: tauri::async_runtime::Receiver<CommandEvent>,
    job_id: u32,
    duration_s: f64,
    output: String,
    cap: Option<u64>,
) {
    let mut percent = 0.0;
    let mut speed = None;
    let mut last_sent = -1.0;
    let mut stderr_tail: VecDeque<String> = VecDeque::new();
    let mut exit_code = None;

    // `while let` keeps looping as long as the channel yields events; it ends when ffmpeg's pipes close.
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => match encoder::parse_progress_line(&String::from_utf8_lossy(&bytes)) {
                Some(ProgressLine::OutTimeUs(us)) => percent = encoder::percent(us, duration_s),
                Some(ProgressLine::Speed(s)) => speed = Some(s),
                // Only bother the UI when the whole-number percentage changes.
                Some(ProgressLine::BlockEnd { .. }) if percent.floor() > last_sent => {
                    last_sent = percent.floor();
                    let _ = app.emit("job-progress", JobProgress { job_id, percent, speed });
                }
                _ => {}
            },
            CommandEvent::Stderr(bytes) => {
                stderr_tail.push_back(String::from_utf8_lossy(&bytes).trim().to_string());
                if stderr_tail.len() > 20 {
                    stderr_tail.pop_front();
                }
            }
            CommandEvent::Terminated(payload) => exit_code = payload.code,
            _ => {}
        }
    }

    // Still in the map → it ended on its own. Gone → `cancel` removed it.
    let jobs = app.state::<Jobs>();
    let cancelled = jobs.running.lock().unwrap().remove(&job_id).is_none();

    if exit_code == Some(0) && !cancelled {
        let size_bytes = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        let over_cap = cap.is_some_and(|c| size_bytes > c);
        let _ = app.emit("job-done", JobDone { job_id, output_path: output, size_bytes, over_cap });
    } else {
        let _ = std::fs::remove_file(&output); // don't leave half-written files behind
        let message = if cancelled {
            "Cancelled".to_string()
        } else {
            let detail = stderr_tail.iter().rev().find(|l| !l.is_empty()).cloned().unwrap_or_default();
            format!("Compression failed: {detail}")
        };
        let _ = app.emit("job-error", JobError { job_id, message, cancelled });
    }
}
