//! Runs ffmpeg in the background, reports progress to the UI, and supports cancelling.
//!
//! Flow: `compress` plans the job and immediately returns a job id. A background task then runs one or
//! more ffmpeg *passes* and emits events the UI listens to: `job-progress`, `job-done`, `job-error`.
//! - If a hardware encoder fails, the pass is re-run with Windows' software encoder.
//! - If a size-limited output comes out too big, it is re-encoded at a corrected bitrate (up to 2 retries).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::{oneshot, OnceCell};

use crate::encoder::{self, ProgressLine, VideoEncoder};
use crate::planner::{self, EncodePlan, Mode};
use crate::presets::{self, Budget};
use crate::probe::VideoInfo;

/// Most passes a job may take: the first encode plus two size-correcting retries.
const MAX_PASSES: u8 = 3;

/// Shared app state for running jobs, registered once with `app.manage(Jobs::default())`.
///
/// Tauri may run commands on several threads at once, so shared data must be thread-safe:
/// - `AtomicU32` is a number that can be incremented safely from any thread.
/// - `Mutex<T>` lets only one thread at a time touch the data inside (`.lock()` waits for its turn).
/// - `OnceCell<T>` is filled exactly once (the first caller runs the detection, everyone else waits for it).
#[derive(Default)]
pub struct Jobs {
    next_id: AtomicU32,
    /// The ffmpeg process currently running for each job.
    running: Mutex<HashMap<u32, CommandChild>>,
    /// Jobs the user cancelled. A set (not just "remove from `running`") because a cancel can arrive
    /// between two passes, when no ffmpeg is running.
    cancelled: Mutex<HashSet<u32>>,
    encoder: OnceCell<VideoEncoder>,
    /// Jobs started and not yet finished (including between passes, when no ffmpeg is running).
    active: AtomicU32,
}

impl Jobs {
    /// True while any compression is in progress; closing the window then hides it instead of destroying it.
    pub fn has_active_jobs(&self) -> bool {
        self.active.load(Ordering::Relaxed) > 0
    }

    fn is_cancelled(&self, job_id: u32) -> bool {
        self.cancelled.lock().unwrap().contains(&job_id)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobProgress {
    job_id: u32,
    percent: f64,
    speed: Option<f64>,
    /// 1 for the first encode; 2-3 while re-encoding to fit the size limit.
    pass: u8,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDone {
    job_id: u32,
    pub output_path: String,
    pub size_bytes: u64,
    /// Which encoder produced the file, e.g. "NVIDIA NVENC" (or "Copied" when nothing was re-encoded).
    encoder: &'static str,
    /// Whether the file was put on the clipboard (single-file jobs from the drop window).
    copied: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobError {
    job_id: u32,
    pub message: String,
    pub cancelled: bool,
}

/// How a job ended. `Result` is Rust's built-in "success or failure" enum: `Ok(JobDone)` or `Err(JobError)`.
pub type JobOutcome = Result<JobDone, JobError>;

/// True while anything is compressing or queued: closing the window then hides it instead of destroying it.
pub fn is_busy(app: &AppHandle) -> bool {
    app.state::<Jobs>().has_active_jobs() || app.state::<crate::queue::Queue>().has_unfinished()
}

/// If the window was only hidden to keep a job's progress, destroy it once there's nothing left to show.
pub fn free_window_if_idle(app: &AppHandle) {
    if is_busy(app) {
        return;
    }
    if let Some(window) = app.get_webview_window(crate::tray::MAIN_WINDOW) {
        if !window.is_visible().unwrap_or(true) {
            let _ = window.destroy();
        }
    }
}

/// Picks the best encoder by trying a 1-frame encode with each hardware encoder in turn.
async fn detect_encoder(app: &AppHandle) -> VideoEncoder {
    for candidate in VideoEncoder::HARDWARE {
        let Ok(command) = app.shell().sidecar("ffmpeg") else { break };
        let works = command
            .args(encoder::probe_encoder_args(candidate))
            .output()
            .await
            .is_ok_and(|out| out.status.success());
        if works {
            return candidate;
        }
    }
    VideoEncoder::MediaFoundation
}

/// The detected encoder; the first call runs detection, later calls reuse the answer.
pub async fn best_encoder(app: &AppHandle) -> VideoEncoder {
    *app.state::<Jobs>().encoder.get_or_init(|| detect_encoder(app)).await
}

/// For the UI: "NVIDIA NVENC", "Windows (software)", ...
#[tauri::command]
pub async fn get_encoder(app: AppHandle) -> &'static str {
    best_encoder(&app).await.label()
}

/// Starts compressing and returns the job id right away; results arrive as events.
#[tauri::command]
pub async fn compress(app: AppHandle, info: VideoInfo, preset_id: String) -> Result<u32, String> {
    // `_outcome` is dropped: the drop window only needs the events.
    let (job_id, _outcome) = start_job(&app, info, &preset_id, true).await?;
    Ok(job_id)
}

/// Plans and starts a job in the background. Besides the usual events, the returned receiver gets the
/// job's outcome once, so a caller (the right-click queue) can `.await` the job finishing.
///
/// `single`: a one-off job from the drop window. Its result is copied to the clipboard and gets its own
/// notification. Queue jobs (`false`) leave the clipboard alone and are summarised by the queue instead.
///
/// `oneshot` is a channel that carries exactly one message: the sender lives in the job, the receiver here.
pub async fn start_job(
    app: &AppHandle,
    info: VideoInfo,
    preset_id: &str,
    single: bool,
) -> Result<(u32, oneshot::Receiver<JobOutcome>), String> {
    let preset = presets::find(preset_id).ok_or_else(|| format!("Unknown preset '{preset_id}'"))?;
    let plan = planner::plan(&info, preset)?;
    let output_dir = chosen_output_dir(app);
    let output = encoder::output_path(Path::new(&info.path), output_dir.as_deref(), preset.suffix, |p| p.exists());
    let cap = match preset.budget {
        Budget::MaxBytes(bytes) => Some(bytes),
        Budget::Quality { .. } => None,
    };
    let encoder = best_encoder(app).await;

    let jobs = app.state::<Jobs>();
    let job_id = jobs.next_id.fetch_add(1, Ordering::Relaxed) + 1;
    let job = Job { job_id, info, plan, output: output.to_string_lossy().into_owned(), cap, encoder, single };

    // Hand the work to a background task so this returns immediately.
    let (sender, receiver) = oneshot::channel();
    jobs.active.fetch_add(1, Ordering::Relaxed);
    tauri::async_runtime::spawn(run_job(app.clone(), job, sender));
    Ok((job_id, receiver))
}

/// The output folder from the tray's "Save to" menu, if one is set and usable. If it was deleted or can't be
/// created (e.g. an unplugged drive), files go next to the original instead of failing.
fn chosen_output_dir(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(crate::settings::current(app).output_dir?);
    std::fs::create_dir_all(&dir).ok().map(|_| dir)
}

/// Stops a job, whether ffmpeg is mid-pass or the job is between passes.
#[tauri::command]
pub fn cancel(jobs: State<'_, Jobs>, job_id: u32) -> Result<(), String> {
    jobs.cancelled.lock().unwrap().insert(job_id);
    let child = jobs.running.lock().unwrap().remove(&job_id);
    match child {
        Some(child) => child.kill().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

/// Everything a background job needs, bundled so it can be moved into the task in one piece.
struct Job {
    job_id: u32,
    info: VideoInfo,
    plan: EncodePlan,
    output: String,
    cap: Option<u64>,
    encoder: VideoEncoder,
    /// A one-off job from the drop window (see `start_job`).
    single: bool,
}

enum PassOutcome {
    Finished,
    Failed(String),
    Cancelled,
}

/// Runs passes until the output is good, the job fails, or it's cancelled.
/// `mut job` = this function owns the job and may change it (e.g. lower the bitrate between passes).
async fn run_job(app: AppHandle, mut job: Job, outcome_sender: oneshot::Sender<JobOutcome>) {
    let mut pass = 1;
    let result: Result<u64, (String, bool)> = loop {
        let args = encoder::build_args(&job.info.path, &job.output, &job.plan, job.encoder);
        match run_pass(&app, job.job_id, args, job.info.duration_s, pass).await {
            PassOutcome::Cancelled => break Err(("Cancelled".into(), true)),
            PassOutcome::Failed(detail) => {
                // Hardware encoders can fail on odd inputs or driver problems: fall back to software once.
                if job.plan.mode == Mode::Encode && job.encoder != VideoEncoder::MediaFoundation {
                    job.encoder = VideoEncoder::MediaFoundation;
                    continue;
                }
                break Err((format!("Compression failed: {detail}"), false));
            }
            PassOutcome::Finished => {}
        }

        let size = std::fs::metadata(&job.output).map(|m| m.len()).unwrap_or(0);
        match job.cap {
            // Too big (only possible when re-encoding: copies are only chosen when they already fit).
            Some(cap) if size > cap && job.plan.mode == Mode::Encode => {
                if pass >= MAX_PASSES {
                    break Err((format!("Couldn't fit this video under {}", presets::format_mb(cap)), false));
                }
                job.plan.video_kbps = encoder::retry_video_kbps(&job.plan, size);
                pass += 1;
            }
            _ => break Ok(size),
        }
    };

    let jobs = app.state::<Jobs>();
    jobs.cancelled.lock().unwrap().remove(&job.job_id);
    jobs.active.fetch_sub(1, Ordering::Relaxed);

    let outcome = match result {
        Ok(size_bytes) => {
            let encoder = if job.plan.mode == Mode::Copy { "Copied" } else { job.encoder.label() };
            let copied = job.single && crate::clipboard::copy_files(&[job.output.clone()]).is_ok();
            if job.single {
                let name = Path::new(&job.output).file_name().map(|n| n.to_string_lossy().into_owned());
                let size = crate::notify::format_size(size_bytes);
                let body = if copied { format!("{size}. Copied, paste it anywhere.") } else { size };
                crate::notify::show(&app, &format!("{} is ready", name.unwrap_or_default()), &body);
            }
            let done = JobDone { job_id: job.job_id, output_path: job.output, size_bytes, encoder, copied };
            let _ = app.emit("job-done", done.clone());
            Ok(done)
        }
        Err((message, cancelled)) => {
            let _ = std::fs::remove_file(&job.output); // never leave a broken or oversized file behind
            if job.single && !cancelled {
                let name = Path::new(&job.info.path).file_name().map(|n| n.to_string_lossy().into_owned());
                crate::notify::show(&app, &format!("Couldn't compress {}", name.unwrap_or_default()), &message);
            }
            let error = JobError { job_id: job.job_id, message, cancelled };
            let _ = app.emit("job-error", error.clone());
            Err(error)
        }
    };
    // Fails only if nobody is waiting for it (the drop window), which is fine.
    let _ = outcome_sender.send(outcome);

    // The window was closed during this job and only hidden to keep its progress: free it now.
    free_window_if_idle(&app);
}

/// One ffmpeg run, streaming progress events until it exits.
async fn run_pass(app: &AppHandle, job_id: u32, args: Vec<String>, duration_s: f64, pass: u8) -> PassOutcome {
    let jobs = app.state::<Jobs>();
    if jobs.is_cancelled(job_id) {
        return PassOutcome::Cancelled;
    }

    let spawned = app.shell().sidecar("ffmpeg").and_then(|command| command.args(args).spawn());
    let (mut rx, child) = match spawned {
        Ok(pair) => pair,
        Err(e) => return PassOutcome::Failed(format!("could not start ffmpeg ({e})")),
    };
    jobs.running.lock().unwrap().insert(job_id, child);
    // A cancel that landed between the check above and the insert found nothing to kill: handle it now.
    if jobs.is_cancelled(job_id) {
        if let Some(child) = jobs.running.lock().unwrap().remove(&job_id) {
            let _ = child.kill();
        }
    }

    let _ = app.emit("job-progress", JobProgress { job_id, percent: 0.0, speed: None, pass });
    let mut percent = 0.0;
    let mut speed = None;
    let mut last_sent = 0.0;
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
                    let _ = app.emit("job-progress", JobProgress { job_id, percent, speed, pass });
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

    jobs.running.lock().unwrap().remove(&job_id);
    if jobs.is_cancelled(job_id) {
        PassOutcome::Cancelled
    } else if exit_code == Some(0) {
        PassOutcome::Finished
    } else {
        let detail = stderr_tail.iter().rev().find(|l| !l.is_empty()).cloned();
        PassOutcome::Failed(detail.unwrap_or_else(|| format!("ffmpeg exited with code {exit_code:?}")))
    }
}
