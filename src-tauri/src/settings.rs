//! The few things Bonsai remembers between runs, saved as a small JSON file in the app's config folder
//! (%APPDATA%\com.bonsai.videocompressor\settings.json).

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Where compressed files go. `None` (the default) = next to the original.
    pub output_dir: Option<String>,
}

/// Managed state: the settings currently in effect.
#[derive(Default)]
pub struct SettingsState(pub Mutex<Settings>);

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("settings.json"))
}

/// Reads the saved settings; a missing or broken file just means defaults.
pub fn load(app: &AppHandle) -> Settings {
    file(app)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn current(app: &AppHandle) -> Settings {
    app.state::<SettingsState>().0.lock().unwrap().clone()
}

/// Changes the output folder (`None` = next to the original) and saves it.
pub fn set_output_dir(app: &AppHandle, dir: Option<String>) {
    let settings = {
        let state = app.state::<SettingsState>();
        let mut settings = state.0.lock().unwrap();
        settings.output_dir = dir;
        settings.clone()
    };
    if let Some(path) = file(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&settings) {
            let _ = std::fs::write(path, json);
        }
    }
}
