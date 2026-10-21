//! The right-click queue: files sent from Explorer are compressed one after another.
//!
//! Each file goes through the same steps as the drop window (probe → plan → job), but driven from Rust.
//! The UI gets the whole list in a `queue-changed` event whenever an item changes state; per-percent progress
//! still comes through the normal `job-progress` events (matched by `jobId`).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{jobs, presets, probe, tray};

/// `tag = "kind"` makes serde write `{ "kind": "done", "outputPath": ..., "sizeBytes": ... }`,
/// which is exactly a TypeScript discriminated union.
#[derive(Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum QueueStatus {
    Waiting,
    Running,
    Done { output_path: String, size_bytes: u64 },
    Failed { message: String },
    Cancelled,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    id: u32,
    path: String,
    preset_id: String,
    preset_label: &'static str,
    status: QueueStatus,
    /// Set once the compression has started, so the UI can match `job-progress` events.
    job_id: Option<u32>,
}

#[derive(Default)]
pub struct Queue {
    items: Mutex<Vec<QueueItem>>,
    next_id: AtomicU32,
    /// Whether the background loop that works through the queue is alive.
    worker_running: AtomicBool,
}

impl Queue {
    /// Anything still waiting or running?
    pub fn has_unfinished(&self) -> bool {
        self.items
            .lock()
            .unwrap()
            .iter()
            .any(|item| matches!(item.status, QueueStatus::Waiting | QueueStatus::Running))
    }
}

fn emit_changed(app: &AppHandle) {
    let items = app.state::<Queue>().items.lock().unwrap().clone();
    let _ = app.emit("queue-changed", items);
}

/// Changes one item (if it still exists) and tells the UI.
fn update(app: &AppHandle, id: u32, change: impl FnOnce(&mut QueueItem)) {
    {
        // The inner block makes the lock end before `emit_changed` takes it again.
        let queue = app.state::<Queue>();
        let mut items = queue.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
            change(item);
        }
    }
    emit_changed(app);
}

/// Adds files to the queue, shows the window, and makes sure the worker is running.
pub fn enqueue(app: &AppHandle, preset_id: &str, paths: Vec<String>) {
    let Some(preset) = presets::find(preset_id) else { return };
    let queue = app.state::<Queue>();
    let start_worker = {
        let mut items = queue.items.lock().unwrap();
        for path in paths {
            let id = queue.next_id.fetch_add(1, Ordering::Relaxed) + 1;
            items.push(QueueItem {
                id,
                path,
                preset_id: preset.id.to_string(),
                preset_label: preset.label,
                status: QueueStatus::Waiting,
                job_id: None,
            });
        }
        // Checked while still holding the lock, so it can't race with the worker deciding to stop.
        // `swap` sets the flag and returns the old value: `false` means no worker was running.
        !queue.worker_running.swap(true, Ordering::SeqCst)
    };
    emit_changed(app);
    tray::show_main_window(app);
    if start_worker {
        tauri::async_runtime::spawn(worker(app.clone()));
    }
}

/// Works through waiting items one at a time until none are left.
async fn worker(app: AppHandle) {
    loop {
        let next = {
            let queue = app.state::<Queue>();
            let mut items = queue.items.lock().unwrap();
            match items.iter_mut().find(|item| item.status == QueueStatus::Waiting) {
                Some(item) => {
                    item.status = QueueStatus::Running;
                    Some((item.id, item.path.clone(), item.preset_id.clone()))
                }
                None => {
                    queue.worker_running.store(false, Ordering::SeqCst);
                    None
                }
            }
        };
        let Some((id, path, preset_id)) = next else { break };
        emit_changed(&app);

        let status = run_item(&app, id, path, &preset_id).await;
        update(&app, id, |item| item.status = status);
    }
    jobs::free_window_if_idle(&app);
}

fn is_cancelled(app: &AppHandle, id: u32) -> bool {
    let queue = app.state::<Queue>();
    let items = queue.items.lock().unwrap();
    items.iter().any(|item| item.id == id && item.status == QueueStatus::Cancelled)
}

async fn run_item(app: &AppHandle, id: u32, path: String, preset_id: &str) -> QueueStatus {
    let info = match probe::probe(app.clone(), path).await {
        Ok(info) => info,
        Err(message) => return QueueStatus::Failed { message },
    };
    // Cancelled while we were reading the file? Don't start compressing it.
    if is_cancelled(app, id) {
        return QueueStatus::Cancelled;
    }

    let (job_id, outcome) = match jobs::start_job(app, info, preset_id).await {
        Ok(started) => started,
        Err(message) => return QueueStatus::Failed { message },
    };
    update(app, id, |item| item.job_id = Some(job_id));
    // A cancel that arrived before we knew the job id couldn't stop the job: do it now.
    if is_cancelled(app, id) {
        let _ = jobs::cancel(app.state::<jobs::Jobs>(), job_id);
    }

    // `.await` on the receiver waits until the job sends its outcome.
    match outcome.await {
        Ok(Ok(done)) => QueueStatus::Done { output_path: done.output_path, size_bytes: done.size_bytes },
        Ok(Err(error)) if error.cancelled => QueueStatus::Cancelled,
        Ok(Err(error)) => QueueStatus::Failed { message: error.message },
        Err(_) => QueueStatus::Failed { message: "The compression stopped unexpectedly".into() },
    }
}

#[tauri::command]
pub fn get_queue(queue: State<'_, Queue>) -> Vec<QueueItem> {
    queue.items.lock().unwrap().clone()
}

/// Cancels one item: a waiting one is simply skipped, a running one has its ffmpeg stopped.
#[tauri::command]
pub fn cancel_queue_item(app: AppHandle, id: u32) {
    let job_to_stop = {
        let queue = app.state::<Queue>();
        let mut items = queue.items.lock().unwrap();
        let Some(item) = items.iter_mut().find(|item| item.id == id) else { return };
        match item.status {
            QueueStatus::Waiting | QueueStatus::Running => {
                item.status = QueueStatus::Cancelled;
                item.job_id
            }
            _ => None,
        }
    };
    if let Some(job_id) = job_to_stop {
        let _ = jobs::cancel(app.state::<jobs::Jobs>(), job_id);
    }
    emit_changed(&app);
}

#[tauri::command]
pub fn cancel_queue(app: AppHandle) {
    let ids: Vec<u32> = app.state::<Queue>().items.lock().unwrap().iter().map(|item| item.id).collect();
    for id in ids {
        cancel_queue_item(app.clone(), id);
    }
}

/// Removes finished items, so the window goes back to the normal drop screen.
#[tauri::command]
pub fn clear_queue(app: AppHandle) {
    app.state::<Queue>()
        .items
        .lock()
        .unwrap()
        .retain(|item| matches!(item.status, QueueStatus::Waiting | QueueStatus::Running));
    emit_changed(&app);
}
