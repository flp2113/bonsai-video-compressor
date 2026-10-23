mod clipboard;
mod encoder;
mod jobs;
mod launch;
mod notify;
mod planner;
mod popup;
mod presets;
mod probe;
mod queue;
mod settings;
mod shell_menu;
mod tray;

use tauri::{Manager, RunEvent, WindowEvent};

/// Passed by the "Start with Windows" entry, so a sign-in start stays quietly in the tray.
const AUTOSTART_ARG: &str = "--autostart";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    launch::wait_for_running_copy(&context.config().identifier);

    let app = tauri::Builder::default()
        // Must be the first plugin. A second launch doesn't start another copy: its arguments arrive here, in
        // the copy already running. That's how each file of a right-click on several files reaches the queue.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            match shell_menu::parse_launch_args(&args) {
                Some((preset, paths)) => queue::enqueue(app, &preset, paths),
                None => tray::show_main_window(app),
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().arg(AUTOSTART_ARG).build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(jobs::Jobs::default())
        .manage(queue::Queue::default())
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(settings::SettingsState(std::sync::Mutex::new(settings::load(&handle))));
            enable_autostart_on_first_run(&handle); // before the tray, so its tick shows the result
            tray::setup(&handle)?;
            register_shell_menu();

            let args: Vec<String> = std::env::args().collect();
            if let Some((preset, paths)) = shell_menu::parse_launch_args(&args) {
                queue::enqueue(&handle, &preset, paths); // started from a right-click
            } else if !args.iter().any(|arg| arg == AUTOSTART_ARG) {
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
                // Alt+F4 on the popup: same as its ✕ during a batch, just tuck it away.
                if window.label() == popup::POPUP_WINDOW {
                    if let Some(popup) = window.app_handle().get_webview_window(popup::POPUP_WINDOW) {
                        if jobs::is_busy(window.app_handle()) {
                            api.prevent_close();
                            popup::hide(&popup);
                        }
                    }
                    return;
                }
                if jobs::is_busy(window.app_handle()) {
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
            jobs::get_encoder,
            queue::get_queue,
            queue::cancel_queue_item,
            queue::cancel_queue,
            queue::clear_queue,
            popup::resize_popup,
            popup::popup_ready,
            popup::hide_popup,
            popup::dismiss_popup,
            clipboard::copy_files_to_clipboard
        ])
        .build(context)
        .expect("error while building tauri application");

    app.run(|_app, event| {
        // When the last window closes Tauri wants to exit (`code: None`): stay alive in the tray instead.
        // "Quit Bonsai" calls `app.exit(0)`, which comes with a code and is let through.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}

/// Adds "Compress with Bonsai" to Explorer's right-click menu, pointing at this exe.
/// Done on every start (dev builds too), so the menu always points at the copy that ran last.
fn register_shell_menu() {
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        if let Err(e) = shell_menu::register(&exe.to_string_lossy()) {
            eprintln!("could not add the right-click menu: {e}");
        }
    }
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
