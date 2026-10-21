//! Windows notifications when compressions finish, shown only when the Bonsai window isn't in front
//! (if it is, the window itself already shows the result).

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

/// The one summary shown when a right-click queue finishes, e.g. ("3 videos ready", "57.4 MB in total").
pub fn queue_summary(done: usize, total_bytes: u64, failed: usize) -> Option<(String, String)> {
    let title = match (done, failed) {
        (0, 0) => return None, // everything was cancelled: nothing to say
        (0, _) => format!("Couldn't compress {}", plural(failed, "video")),
        (1, _) => "1 video ready".to_string(),
        (n, _) => format!("{n} videos ready"),
    };
    let mut body = if done > 0 { format!("{} in total", format_size(total_bytes)) } else { String::new() };
    if done > 0 && failed > 0 {
        body.push_str(&format!(", {} failed", failed));
    }
    if done == 0 {
        body = "Open Bonsai to see what went wrong".into();
    }
    Some((title, body))
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
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

    #[test]
    fn queue_summary_texts() {
        assert_eq!(
            queue_summary(3, 58_000_000, 0),
            Some(("3 videos ready".into(), "58.0 MB in total".into()))
        );
        assert_eq!(
            queue_summary(1, 19_300_000, 2),
            Some(("1 video ready".into(), "19.3 MB in total, 2 failed".into()))
        );
        assert_eq!(
            queue_summary(0, 0, 2),
            Some(("Couldn't compress 2 videos".into(), "Open Bonsai to see what went wrong".into()))
        );
        assert_eq!(queue_summary(0, 0, 0), None);
    }
}
