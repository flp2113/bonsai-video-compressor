//! The tray icon, and showing the main window on demand.
//!
//! Bonsai lives in the tray. Closing the window destroys its webview (that's where most of the memory goes);
//! clicking the tray icon builds a fresh window from the config in `tauri.conf.json`.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;

use crate::settings;

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

const TRAY_ID: &str = "bonsai";

/// Builds the tray menu from the current state (Start with Windows, output folder).
/// It's rebuilt after every change, so the ticks always show the real state, even if a change failed.
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let output_dir = settings::current(app).output_dir;

    // "Save to" submenu: next to the original (default), or the chosen folder.
    let next_to_original = CheckMenuItem::with_id(
        app,
        "save-next",
        "Next to the original",
        true,
        output_dir.is_none(),
        None::<&str>,
    )?;
    let choose = MenuItem::with_id(app, "save-choose", "Choose folder…", true, None::<&str>)?;
    let save_to = Submenu::with_id(app, "save-to", "Save to", true)?;
    save_to.append(&next_to_original)?;
    if let Some(dir) = &output_dir {
        save_to.append(&CheckMenuItem::with_id(app, "save-folder", dir, true, true, None::<&str>)?)?;
    }
    save_to.append(&PredefinedMenuItem::separator(app)?)?;
    save_to.append(&choose)?;

    let open = MenuItem::with_id(app, "open", "Open Bonsai", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Bonsai", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    Menu::with_items(app, &[&open, &save_to, &autostart, &separator, &quit])
}

fn refresh_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn toggle_autostart(app: &AppHandle) {
    let manager = app.autolaunch();
    let wanted = !manager.is_enabled().unwrap_or(false);
    let result = if wanted { manager.enable() } else { manager.disable() };
    if let Err(e) = result {
        eprintln!("could not change Start with Windows: {e}");
    }
}

/// Opens a folder picker; the dialog runs in the background and calls us back with the answer.
fn choose_output_folder(app: &AppHandle) {
    let mut dialog = app.dialog().file().set_title("Save compressed videos to");
    if let Some(dir) = settings::current(app).output_dir {
        dialog = dialog.set_directory(dir);
    }
    // `app.clone()` because the callback runs later, after this function has returned.
    let app = app.clone();
    dialog.pick_folder(move |folder| {
        // `None` = the user closed the picker without choosing: keep the current setting.
        if let Some(path) = folder.and_then(|f| f.into_path().ok()) {
            settings::set_output_dir(&app, Some(path.to_string_lossy().into_owned()));
        }
        refresh_menu(&app);
    });
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Bonsai")
        .menu(&build_menu(app)?)
        // Left-click opens the window; the menu is on right-click.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "open" => show_main_window(app),
                "autostart" => toggle_autostart(app),
                "save-next" => settings::set_output_dir(app, None),
                "save-choose" => return choose_output_folder(app), // refreshes once the picker closes
                "quit" => return app.exit(0),
                _ => {}
            }
            // Clicking a check item flips its tick by itself; rebuilding puts back the real state.
            refresh_menu(app);
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
