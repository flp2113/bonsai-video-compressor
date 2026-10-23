//! Windows notifications when a drop-window compression finishes, shown only when the Bonsai window isn't in
//! front (if it is, the window itself already shows the result). Right-click batches use the popup instead.

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::tray::MAIN_WINDOW;

/// Same units as the UI (src/lib/format.ts): decimal MB, one decimal below 100 MB.
pub fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_000_000.0;
    if mb >= 1000.0 {
        format!("{:.1} GB", mb / 1000.0)
    } else if mb >= 100.0 {
        format!("{} MB", mb.round())
    } else {
        format!("{mb:.1} MB")
    }
}

fn window_in_front(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN_WINDOW)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

pub fn show(app: &AppHandle, title: &str, body: &str) {
    if window_in_front(app) {
        return;
    }
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("could not show a notification: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_match_the_ui() {
        assert_eq!(format_size(19_336_657), "19.3 MB");
        assert_eq!(format_size(152_481_662), "152 MB");
        assert_eq!(format_size(2_400_000_000), "2.4 GB");
    }
}
