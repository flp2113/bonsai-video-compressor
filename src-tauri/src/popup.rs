//! The small popup in the bottom-right corner that shows a right-click batch: live progress, then a summary.
//!
//! It looks and behaves like a notification: no title bar, always on top, not in the taskbar, and it never
//! takes the keyboard focus from the app you're using. The page itself is `src/routes/popup/+page.svelte`;
//! it tells us its height (`resize_popup`) so the window always hugs its content above the taskbar.
//!
//! The window is transparent and the page draws the card (with its shadow) inside a 12 px transparent border.
//! That's what lets the card really fade in and out: an opaque window would leave a grey box behind.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use crate::queue::Queue;

pub const POPUP_WINDOW: &str = "popup";

/// Width in logical pixels (before Windows' display scaling), including the page's 12 px transparent border on
/// each side. The height comes from the page.
const WIDTH: f64 = 360.0;
/// Height used until the page reports its real one.
const START_HEIGHT: f64 = 120.0;
/// Gap between the window and the screen edge / taskbar. Zero, because the page's transparent border already
/// keeps the visible card 12 px away from them.
const MARGIN: f64 = 0.0;

/// True while `show_popup` is building the window (same re-entrancy guard as `tray::show_main_window`).
static OPENING: AtomicBool = AtomicBool::new(false);

/// Whether there's something for the popup to show: a batch in progress, or a summary not yet dismissed.
pub fn is_active(app: &AppHandle) -> bool {
    app.state::<Queue>().has_items()
}

/// Shows the popup without focusing it, creating it if needed.
pub fn show_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(POPUP_WINDOW) {
        if !is_shown(&window) {
            show_without_focus(&window);
            // The page faded its card out before it was hidden; this tells it to fade back in.
            let _ = app.emit_to(POPUP_WINDOW, "popup-shown", ());
        }
        return;
    }
    if OPENING.swap(true, Ordering::SeqCst) {
        return;
    }
    // Built hidden: the page calls `popup_ready` once it has drawn and sized itself, so there's no blank flash.
    let built = WebviewWindowBuilder::new(app, POPUP_WINDOW, WebviewUrl::App("popup".into()))
        .title("Bonsai")
        .inner_size(WIDTH, START_HEIGHT)
        .decorations(false)
        .transparent(true)
        // The page draws the card's shadow; Windows' own would outline the whole (invisible) window.
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build();
    match built {
        Ok(window) => place(&window, START_HEIGHT),
        Err(e) => eprintln!("could not open the popup: {e}"),
    }
    OPENING.store(false, Ordering::SeqCst);
}

/// `window.show()` would also activate the window (stealing the focus), so on Windows we ask for
/// SW_SHOWNOACTIVATE: "show it, but leave the focus where it is".
fn show_without_focus(window: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
        // `unsafe`: a plain Win32 call on a window handle Tauri just gave us.
        unsafe { ShowWindow(hwnd.0 as _, SW_SHOWNOACTIVATE) };
        return;
    }
    let _ = window.show();
}

/// Whether the popup is on screen. Asked of Windows directly, for the same reason as `hide` below.
fn is_shown(window: &WebviewWindow) -> bool {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;
        return unsafe { IsWindowVisible(hwnd.0 as _) } != 0;
    }
    window.is_visible().unwrap_or(false)
}

/// Hides the popup. It must be hidden the same way it was shown: Tauri keeps its own "visible" flag, which
/// never saw the Win32 show above, so `window.hide()` would think it's already hidden and do nothing.
pub fn hide(window: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        unsafe { ShowWindow(hwnd.0 as _, SW_HIDE) };
        return;
    }
    let _ = window.hide();
}

/// Where the popup's top-left corner goes so its bottom-right corner sits `margin` inside the work area
/// (the screen minus the taskbar). All values are physical pixels. Pure, so it's unit-tested.
fn corner_position(area: (i32, i32, u32, u32), size: (u32, u32), margin: u32) -> (i32, i32) {
    let (area_x, area_y, area_w, area_h) = area;
    let (w, h) = size;
    // `as i64` first, so a popup taller than the screen gives a negative offset instead of wrapping around.
    let x = area_x as i64 + area_w as i64 - w as i64 - margin as i64;
    let y = area_y as i64 + area_h as i64 - h as i64 - margin as i64;
    // Never above or left of the work area, even if the content is huge.
    (x.max(area_x as i64) as i32, y.max(area_y as i64) as i32)
}

/// Resizes the window to `height` logical pixels and pins it to the bottom-right corner of the main screen
/// (where the tray is).
fn place(window: &WebviewWindow, height: f64) {
    let Ok(Some(monitor)) = window.primary_monitor() else { return };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let size = ((WIDTH * scale).round() as u32, (height * scale).round() as u32);
    let area = (area.position.x, area.position.y, area.size.width, area.size.height);
    let (x, y) = corner_position(area, size, (MARGIN * scale).round() as u32);
    let _ = window.set_size(PhysicalSize::new(size.0, size.1));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// The page's height changed (expanded, collapsed, a file finished...): resize and stay anchored.
#[tauri::command]
pub fn resize_popup(window: WebviewWindow, height: f64) {
    place(&window, height);
}

/// The page has drawn itself for the first time: now it can appear.
#[tauri::command]
pub fn popup_ready(window: WebviewWindow) {
    show_without_focus(&window);
}

/// ✕ during a batch: tuck the popup away; the batch carries on and the tray icon brings it back.
#[tauri::command]
pub fn hide_popup(window: WebviewWindow) {
    hide(&window);
}

/// The summary was closed or timed out: forget the finished files and free the window's memory.
/// After this a tray click opens the main window again.
#[tauri::command]
pub fn dismiss_popup(app: AppHandle, window: WebviewWindow) {
    crate::queue::clear_queue(app);
    let _ = window.destroy();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sits_in_the_bottom_right_corner_above_the_taskbar() {
        // 1920×1080 screen with a 48 px taskbar at the bottom.
        assert_eq!(corner_position((0, 0, 1920, 1032), (360, 96), 12), (1548, 924));
    }

    #[test]
    fn follows_a_screen_that_does_not_start_at_zero() {
        // Taskbar at the top (work area starts lower down), on a screen left of the main one.
        assert_eq!(corner_position((-1920, 48, 1920, 1032), (360, 96), 12), (-372, 972));
    }

    #[test]
    fn never_goes_above_the_work_area() {
        assert_eq!(corner_position((0, 0, 1920, 1032), (360, 2000), 12), (1548, 0));
    }
}
