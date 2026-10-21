mod encoder;
mod jobs;
mod planner;
mod presets;
mod probe;
mod tray;

use tauri::{Manager, RunEvent, WindowEvent};

/// Passed by the "Start with Windows" entry, so a sign-in start stays quietly in the tray.
const AUTOSTART_ARG: &str = "--autostart";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Must be the first plugin: a second launch just shows the window of the copy already running.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main_window(app)))
        .plugin(tauri_plugin_autostart::Builder::new().arg(AUTOSTART_ARG).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(jobs::Jobs::default())
        .setup(|app| {
            let handle = app.handle().clone();
            enable_autostart_on_first_run(&handle); // before the tray, so its tick shows the result
            tray::setup(&handle)?;

            let started_at_sign_in = std::env::args().any(|arg| arg == AUTOSTART_ARG);
            if !started_at_sign_in {
                tray::show_main_window(&handle);
            }

            // Detect the GPU encoder in the background at startup, so the first compression doesn't wait for it.
            tauri::async_runtime::spawn(async move {
                jobs::best_encoder(&handle).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window normally destroys it (freeing the webview's memory) while the app stays in
            // the tray. During a compression we only hide it, so its progress is still there when reopened.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<jobs::Jobs>().has_active_jobs() {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            probe::probe,
            presets::list_presets,
            planner::plan_compression,
            jobs::compress,
            jobs::cancel,
            jobs::get_encoder
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app, event| {
        // When the last window closes Tauri wants to exit (`code: None`): stay alive in the tray instead.
        // "Quit Bonsai" calls `app.exit(0)`, which comes with a code and is let through.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}

/// Turns "Start with Windows" on the very first time Bonsai runs; after that the user's choice in the tray
/// menu is kept. Only in release builds, so `npm run tauri dev` never registers the debug exe.
fn enable_autostart_on_first_run(app: &tauri::AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(dir) = app.path().app_config_dir() else { return };
    let marker = dir.join("autostart-default-applied");
    if marker.exists() {
        return;
    }
    use tauri_plugin_autostart::ManagerExt;
    if app.autolaunch().enable().is_ok() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&marker, "");
    }
}
