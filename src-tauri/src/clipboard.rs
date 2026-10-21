//! Copies files to the clipboard the way Explorer's Ctrl+C does, so Ctrl+V in Discord, a chat app or a folder
//! pastes the actual video files (not their paths as text).
//!
//! Windows calls this clipboard format CF_HDROP: a small `DROPFILES` header followed by the file paths in
//! UTF-16, each ending in a 0, with one extra 0 at the very end.

/// The clipboard data for `paths`: header + "path1\0path2\0\0" as UTF-16. Pure, so it's unit-tested.
#[cfg(windows)]
fn hdrop_bytes(paths: &[String]) -> Vec<u8> {
    use windows_sys::Win32::UI::Shell::DROPFILES;

    let header_size = std::mem::size_of::<DROPFILES>();
    let mut wide: Vec<u16> = Vec::new();
    for path in paths {
        wide.extend(path.encode_utf16());
        wide.push(0);
    }
    wide.push(0);

    let header = DROPFILES {
        pFiles: header_size as u32, // where the file list starts, counted from the start of the data
        pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
        fNC: 0,
        fWide: 1, // the paths are UTF-16
    };
    let mut bytes = Vec::with_capacity(header_size + wide.len() * 2);
    // Copy the header struct's raw bytes. `unsafe` because we read a struct as plain bytes; DROPFILES is
    // a plain C struct of numbers, so that's well-defined.
    bytes.extend_from_slice(unsafe {
        std::slice::from_raw_parts(&header as *const DROPFILES as *const u8, header_size)
    });
    for unit in wide {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// Puts the files on the clipboard. Replaces whatever was there.
#[cfg(windows)]
pub fn copy_files(paths: &[String]) -> Result<(), String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Ole::{CF_HDROP, DROPEFFECT_COPY};

    if paths.is_empty() {
        return Ok(());
    }

    /// Copies `data` into a block of memory the clipboard can take ownership of.
    unsafe fn global_copy(data: &[u8]) -> Result<*mut core::ffi::c_void, String> {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, data.len());
            if handle.is_null() {
                return Err("out of memory".into());
            }
            let target = GlobalLock(handle) as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), target, data.len());
            GlobalUnlock(handle);
            Ok(handle)
        }
    }

    // Another app may be holding the clipboard for a moment: retry briefly.
    let mut opened = false;
    for _ in 0..10 {
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    if !opened {
        return Err("The clipboard is busy, try again".into());
    }

    let result = unsafe {
        EmptyClipboard();
        let files = global_copy(&hdrop_bytes(paths))?;
        if SetClipboardData(CF_HDROP as u32, files).is_null() {
            GlobalFree(files);
            Err("Could not copy to the clipboard".to_string())
        } else {
            // Tells Explorer that pasting should copy the files, not move them.
            let name: Vec<u16> = "Preferred DropEffect\0".encode_utf16().collect();
            let format = RegisterClipboardFormatW(name.as_ptr());
            if let Ok(effect) = global_copy(&DROPEFFECT_COPY.to_le_bytes()) {
                if SetClipboardData(format, effect).is_null() {
                    GlobalFree(effect);
                }
            }
            Ok(())
        }
    };
    unsafe { CloseClipboard() };
    result
}

#[cfg(not(windows))]
pub fn copy_files(_paths: &[String]) -> Result<(), String> {
    Err("Copying files is only supported on Windows for now".into())
}

/// For the UI's "Copy" buttons.
#[tauri::command]
pub fn copy_files_to_clipboard(paths: Vec<String>) -> Result<(), String> {
    copy_files(&paths)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn hdrop_lists_paths_as_double_terminated_utf16() {
        let bytes = hdrop_bytes(&["C:\\a.mp4".into(), "C:\\b.mp4".into()]);
        let header = 20; // DROPFILES is 5 × 4 bytes
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), header as u32);
        let text: Vec<u16> =
            bytes[header..].chunks(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
        let expected: Vec<u16> = "C:\\a.mp4\0C:\\b.mp4\0\0".encode_utf16().collect();
        assert_eq!(text, expected);
    }

    /// Replaces your clipboard! Run with `cargo test -- --ignored`, then check with PowerShell:
    /// `Get-Clipboard -Format FileDropList`
    #[test]
    #[ignore]
    fn real_clipboard_copy() {
        let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "\\Cargo.toml").to_string();
        copy_files(&[manifest]).unwrap();
    }
}
