//! Holds the cursor inside the game's window while the mouse is the game's.
//! The sandboxed engine may move the cursor but not clip it (an
//! AppContainer is refused `ClipCursor`), so this side, outside the
//! sandbox, clips by the same rule the engine reads the mouse by
//! (`vilocity-hle` gameinput `mouse_lock`): a click in the window takes the
//! mouse, Esc or another window in front gives it back.
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ClipCursor, GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
};

/// Whether the mouse is the game's after this look (the engine's rule).
fn lock(locked: bool, front: bool, click_inside: bool, esc: bool) -> bool {
    front && !esc && (locked || click_inside)
}

/// Clips for the game process `pid` until `done` is set; frees the cursor at the end.
pub fn watch(pid: u32, done: Arc<AtomicBool>) {
    if std::env::var_os("VILOCITY_MOUSE").is_some_and(|v| v == "0") {
        return;
    }
    let mut locked = false;
    while !done.load(Ordering::Relaxed) {
        // SAFETY: plain queries and a clip, on valid structs.
        unsafe {
            let window = GetForegroundWindow();
            let mut owner = 0;
            GetWindowThreadProcessId(window, &mut owner);
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            let mut at = POINT { x: 0, y: 0 };
            let front = owner == pid && GetWindowRect(window, &mut r) != 0 && GetCursorPos(&mut at) != 0;
            // Inside the frame, as the engine clips (title bar and edges out of reach).
            let inner = RECT { left: r.left + 16, top: r.top + 40, right: r.right - 16, bottom: r.bottom - 16 };
            let inside = front && at.x >= inner.left && at.x < inner.right && at.y >= inner.top && at.y < inner.bottom;
            let now = lock(locked, front, inside && GetAsyncKeyState(0x01) < 0, GetAsyncKeyState(0x1B) < 0);
            if now {
                ClipCursor(&inner);
            } else if locked {
                ClipCursor(std::ptr::null());
            }
            locked = now;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    if locked {
        // SAFETY: a null rectangle frees the cursor.
        unsafe { ClipCursor(std::ptr::null()) };
    }
}

#[cfg(test)]
mod tests {
    use super::lock;

    #[test]
    fn same_rule_as_the_engine() {
        assert!(lock(false, true, true, false));
        assert!(lock(true, true, false, false));
        assert!(!lock(true, true, false, true));
        assert!(!lock(true, false, false, false));
        assert!(!lock(false, true, false, false));
    }
}
