mod encoder;
mod jobs;
mod planner;
mod presets;
mod probe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .manage(jobs::Jobs::default())
        .setup(|app| {
            // Detect the GPU encoder in the background at startup, so the first compression doesn't wait for it.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                jobs::best_encoder(&handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            probe::probe,
            presets::list_presets,
            planner::plan_compression,
            jobs::compress,
            jobs::cancel,
            jobs::get_encoder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
