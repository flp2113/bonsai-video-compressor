mod planner;
mod presets;
mod probe;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            probe::probe,
            presets::list_presets,
            planner::plan_compression
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
