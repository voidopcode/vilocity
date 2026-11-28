//! `vilocity-launcher.exe`: opens the launcher window (`scripts\launcher.ps1`)
//! with no console: the PowerShell that draws it runs without a window of its
//! own. Built with `vilocity.exe` (`cargo build --release -p vilocity`); the
//! launcher puts a "Vilocity" shortcut to it in the repository's folder.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::Command;

/// `scripts\launcher.ps1` in the first folder above `exe` that has it
/// (`target\release\` sits two levels under the repository).
fn script_above(exe: &Path) -> Option<PathBuf> {
    exe.ancestors().skip(1).map(|dir| dir.join("scripts").join("launcher.ps1")).find(|p| p.is_file())
}

#[cfg(windows)]
fn main() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().unwrap_or_default();
    let started = script_above(&exe).ok_or_else(|| "scripts\\launcher.ps1 was not found above this program".to_owned()).and_then(|script| {
        // No console through Windows' own flag, the script policy through PowerShell's own
        // variable: the "-ExecutionPolicy Bypass -WindowStyle Hidden" command line made
        // Kaspersky remove this program as soon as it was opened.
        Command::new("powershell.exe")
            .args(["-NoProfile", "-File"])
            .arg(&script)
            .env("PSExecutionPolicyPreference", "Bypass")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(drop)
            .map_err(|e| format!("PowerShell did not start: {e}"))
    });
    if let Err(why) = started {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (text, title) = (wide(&format!("The Vilocity launcher could not open: {why}.")), wide("Vilocity"));
        // SAFETY: both strings are NUL-terminated and live through the call.
        unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The Vilocity launcher is for Windows.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_is_found_from_target_release() {
        let root = std::env::temp_dir().join(format!("vilocity-launcher-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::create_dir_all(root.join("target").join("release")).unwrap();
        std::fs::write(root.join("scripts").join("launcher.ps1"), "").unwrap();
        let exe = root.join("target").join("release").join("vilocity-launcher.exe");
        assert_eq!(script_above(&exe), Some(root.join("scripts").join("launcher.ps1")));
        assert_eq!(script_above(&std::env::temp_dir().join("nowhere").join("x.exe")), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
