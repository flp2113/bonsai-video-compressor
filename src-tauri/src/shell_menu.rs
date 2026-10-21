//! The Explorer right-click menu: "Compress with Bonsai" → one entry per preset.
//!
//! Classic menu entries are just registry keys. We write them under HKEY_CURRENT_USER, so no admin rights are
//! needed and only the current user is affected. On Windows 11 they appear under "Show more options".
//!
//! When several files are selected, Explorer runs our command once per file:
//! `bonsai.exe --preset discord-free "C:\clips\a.mp4"`. The single-instance plugin funnels all those launches
//! into the one running app, which queues them (see `queue.rs`).

use crate::presets::{Budget, PRESETS};

/// File types that get the menu. Keep in sync with `VIDEO_EXTENSIONS` in src/lib/types.ts.
pub const VIDEO_EXTENSIONS: [&str; 9] = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "wmv", "flv", "ts"];

/// Under HKCU\Software\Classes (the per-user part of HKEY_CLASSES_ROOT).
const CLASSES: &str = r"Software\Classes";
/// The shared submenu every extension points to.
const SUBMENU_KEY: &str = "Bonsai.Compress";

/// One registry value to write: `name` = `value` in `key` (an empty name is the key's default value).
#[derive(Debug, PartialEq)]
pub struct RegValue {
    pub key: String,
    pub name: &'static str,
    pub value: String,
}

fn reg(key: String, name: &'static str, value: impl Into<String>) -> RegValue {
    RegValue { key, name, value: value.into() }
}

/// Every registry value the menu needs, for the given path to bonsai.exe. Pure, so it's unit-tested.
pub fn entries(exe: &str) -> Vec<RegValue> {
    let mut out = Vec::new();

    for ext in VIDEO_EXTENSIONS {
        // SystemFileAssociations adds the entry for this file type whatever app normally opens it.
        let key = format!(r"{CLASSES}\SystemFileAssociations\.{ext}\shell\Bonsai");
        out.push(reg(key.clone(), "MUIVerb", "Compress with Bonsai"));
        out.push(reg(key.clone(), "Icon", format!("\"{exe}\",0")));
        // An empty SubCommands + ExtendedSubCommandsKey = "the submenu lives in that other key".
        out.push(reg(key.clone(), "SubCommands", ""));
        out.push(reg(key.clone(), "ExtendedSubCommandsKey", SUBMENU_KEY));
        // "Player" keeps the entry visible with up to 100 files selected (the default hides it above 15).
        out.push(reg(key, "MultiSelectModel", "Player"));
    }

    // `enumerate()` yields (index, item) pairs, like `array.entries()` in JS.
    for (i, preset) in PRESETS.iter().enumerate() {
        // Explorer sorts entries by key name, so a number prefix keeps the presets table order.
        let key = format!(r"{CLASSES}\{SUBMENU_KEY}\shell\{:02}-{}", i + 1, preset.id);
        let label = match preset.budget {
            Budget::MaxBytes(bytes) => format!("{} ({})", preset.label, crate::presets::format_mb(bytes)),
            Budget::Quality { .. } => preset.label.to_string(),
        };
        out.push(reg(key.clone(), "MUIVerb", label));
        out.push(reg(key.clone(), "MultiSelectModel", "Player"));
        out.push(reg(format!(r"{key}\command"), "", format!("\"{exe}\" --preset {} \"%1\"", preset.id)));
    }
    out
}

/// Writes the menu. Safe to call on every start: it just overwrites the same values (and fixes the exe path
/// if the app moved). The uninstaller removes the keys again (src-tauri/windows/hooks.nsh).
#[cfg(windows)]
pub fn register(exe: &str) -> windows_registry::Result<()> {
    let root = windows_registry::CURRENT_USER;
    for value in entries(exe) {
        root.create(&value.key)?.set_string(value.name, &value.value)?;
    }
    Ok(())
}

/// Reads `--preset <id> <file>...` from the command line. `None` if it's a normal launch (or an unknown preset).
///
/// `args[0]` is the program itself, as in `process.argv` minus the "node" part.
pub fn parse_launch_args(args: &[String]) -> Option<(String, Vec<String>)> {
    let mut preset = None;
    let mut paths = Vec::new();
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        if arg == "--preset" {
            preset = rest.next().cloned(); // the value right after the flag
        } else if !arg.starts_with("--") {
            paths.push(arg.clone());
        }
    }
    // `?` on an Option returns None early, like optional chaining that bails out of the whole function.
    let preset = preset.filter(|id| crate::presets::find(id).is_some())?;
    if paths.is_empty() {
        return None;
    }
    Some((preset, paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Program Files\Bonsai\bonsai.exe";

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn every_extension_gets_a_submenu_entry() {
        let all = entries(EXE);
        for ext in VIDEO_EXTENSIONS {
            let key = format!(r"Software\Classes\SystemFileAssociations\.{ext}\shell\Bonsai");
            let names: Vec<_> = all.iter().filter(|v| v.key == key).map(|v| v.name).collect();
            assert_eq!(names, ["MUIVerb", "Icon", "SubCommands", "ExtendedSubCommandsKey", "MultiSelectModel"]);
        }
    }

    #[test]
    fn submenu_follows_the_presets_table_with_quoted_commands() {
        let commands: Vec<_> = entries(EXE).into_iter().filter(|v| v.key.ends_with(r"\command")).collect();
        assert_eq!(commands.len(), PRESETS.len());
        assert_eq!(commands[0].key, r"Software\Classes\Bonsai.Compress\shell\01-discord-free\command");
        assert_eq!(commands[0].value, format!("\"{EXE}\" --preset discord-free \"%1\""));
        // Keys sort in table order.
        let keys: Vec<_> = commands.iter().map(|c| c.key.clone()).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn size_capped_presets_show_their_limit() {
        let all = entries(EXE);
        let label = |id: &str| {
            all.iter()
                .find(|v| v.key.ends_with(&format!("-{id}")) && v.name == "MUIVerb")
                .map(|v| v.value.clone())
                .unwrap()
        };
        assert_eq!(label("discord-free"), "Discord Free (20 MB)");
        assert_eq!(label("instagram"), "Instagram");
    }

    /// The uninstaller lists the keys by hand (NSIS can't read our Rust), so check it removes all of them.
    #[test]
    fn uninstaller_removes_every_menu_key_and_the_autostart_entry() {
        // `include_str!` pastes the file's text in at compile time, like importing a file `?raw` in Vite.
        let hooks = include_str!("../windows/hooks.nsh");
        for ext in VIDEO_EXTENSIONS {
            let line = format!(r#"DeleteRegKey HKCU "{CLASSES}\SystemFileAssociations\.{ext}\shell\Bonsai""#);
            assert!(hooks.contains(&line), "hooks.nsh is missing: {line}");
        }
        assert!(hooks.contains(&format!(r#"DeleteRegKey HKCU "{CLASSES}\{SUBMENU_KEY}""#)));
        // The autostart plugin names its Run value after the product name.
        assert!(include_str!("../tauri.conf.json").contains(r#""productName": "Bonsai""#));
        assert!(hooks.contains(r#"CurrentVersion\Run" "Bonsai""#));
    }

    #[test]
    fn parses_a_right_click_launch() {
        let parsed = parse_launch_args(&args(&["bonsai.exe", "--preset", "x", r"C:\a b\clip.mov"]));
        assert_eq!(parsed, Some(("x".into(), vec![r"C:\a b\clip.mov".into()])));
    }

    #[test]
    fn ignores_normal_and_unknown_launches() {
        assert_eq!(parse_launch_args(&args(&["bonsai.exe"])), None);
        assert_eq!(parse_launch_args(&args(&["bonsai.exe", "--autostart"])), None);
        assert_eq!(parse_launch_args(&args(&["bonsai.exe", "--preset", "nope", "a.mp4"])), None);
        assert_eq!(parse_launch_args(&args(&["bonsai.exe", "--preset", "x"])), None);
    }
}
