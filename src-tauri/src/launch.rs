//! Makes simultaneous launches (right-click on several files) end up in one running copy of Bonsai.
//!
//! `tauri-plugin-single-instance` forwards a launch to the running copy by finding a hidden message window
//! that copy creates during startup. If several launches arrive while the first copy is still starting,
//! that window doesn't exist yet and the plugin lets them all start as separate copies.
//!
//! The fix: the very first thing each process does is create a named mutex. Only one process can create it
//! (Windows guarantees that atomically); every later one knows a copy is running or starting, and waits until
//! the plugin's message window appears before carrying on, so the plugin then forwards its file.

#[cfg(windows)]
pub fn wait_for_running_copy(identifier: &str) {
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    // Windows APIs want UTF-16 strings ending in a 0, not Rust's UTF-8 `String`.
    fn wide(s: String) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let mutex_name = wide(format!("{identifier}-launch"));
    // `unsafe` marks calls into C APIs that Rust can't check; here we only pass valid, 0-terminated strings.
    // The handle is deliberately never closed: the mutex must live as long as this process does.
    let _handle = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return; // we're the first copy
    }

    // Names used by tauri-plugin-single-instance for its message window.
    let class = wide(format!("{identifier}-sic"));
    let window = wide(format!("{identifier}-siw"));
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if !unsafe { FindWindowW(class.as_ptr(), window.as_ptr()) }.is_null() {
            return; // the first copy is ready to receive our file
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(not(windows))]
pub fn wait_for_running_copy(_identifier: &str) {}
