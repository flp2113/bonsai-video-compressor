//! The tray icon, and showing the main window on demand.
//!
//! Bonsai lives in the tray. Closing the window destroys its webview (that's where most of the memory goes);
//! clicking the tray icon builds a fresh window from the config in `tauri.conf.json`.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

pub const MAIN_WINDOW: &str = "main";

/// Shows the main window, creating it if it was closed (destroyed) earlier.
pub fn show_main_window(app: &AppHandle) {
    // `if let Some(x) = ...` runs the block only when the Option holds a value (the window exists).
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    // Building the webview lets Windows process other messages meanwhile, so this function can be re-entered
    // (e.g. the next right-clicked file arriving) before the window exists. A second window with the same
    // label would crash, so later calls just return: the window being built will show up anyway.
    // `swap(true)` sets the flag and returns its previous value in one step.
    if OPENING.swap(true, Ordering::SeqCst) {
        return;
    }
    // `windows[0]` is the window described in tauri.conf.json (it has `"create": false`, so Tauri
    // doesn't build it at startup: we do, here, when it's wanted).
    if let Some(config) = app.config().app.windows.first() {
        match WebviewWindowBuilder::from_config(app, config).and_then(|builder| builder.build()) {
            Ok(window) => {
                let _ = window.set_focus();
            }
            Err(e) => eprintln!("could not open the window: {e}"),
        }
    }
    OPENING.store(false, Ordering::SeqCst);
}

/// True while `show_main_window` is building the window.
static OPENING: AtomicBool = AtomicBool::new(false);

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);

    let open = MenuItem::with_id(app, "open", "Open Bonsai", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bonsai", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &autostart, &PredefinedMenuItem::separator(app)?, &quit])?;

    let mut tray = TrayIconBuilder::with_id("bonsai")
        .tooltip("Bonsai")
        .menu(&menu)
        // Left-click opens the window; the menu is on right-click.
        .show_menu_on_left_click(false)
        // `move` gives the closure its own copy of `autostart`, since it runs later, after `setup` returns.
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main_window(app),
            "autostart" => {
                let manager = app.autolaunch();
                let wanted = !manager.is_enabled().unwrap_or(false);
                let result = if wanted { manager.enable() } else { manager.disable() };
                if let Err(e) = result {
                    eprintln!("could not change Start with Windows: {e}");
                }
                // Show the real state, so the tick never lies if the change failed.
                let _ = autostart.set_checked(manager.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
