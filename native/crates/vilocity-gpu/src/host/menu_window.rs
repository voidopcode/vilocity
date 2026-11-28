//! (Vilocity) The in-game menu's own window: a layered, topmost popup over the game's window
//! that shows the menu at the screen's own pixels and takes the mouse's clicks (2026-11-28:
//! the menu drawn into the game's picture was scaled with that picture to the window, and its
//! text blurred). It never becomes the active window, so the game's window keeps the keyboard
//! (the menu reads its keys there, `menu::Menu::frame`). Its own thread runs its messages;
//! clicks and wheel turns are queued for the menu, which handles them on the presenting thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC, CreateDIBSection,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HTCLIENT, IDC_ARROW, LoadCursorW, MA_NOACTIVATE, MSG,
    HWND_TOPMOST, PostMessageW, RegisterClassW, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SetCursor, SetWindowPos,
    ShowWindow, TranslateMessage, ULW_ALPHA,
    UpdateLayeredWindow, WM_APP, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_SETCURSOR, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::w;

/// What the mouse did on the menu (window pixels, from its top left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Input {
    Click { x: i32, y: i32, right: bool },
    /// Notches: up positive.
    Wheel(i32),
}

/// The picture to show (BGRA, alpha premultiplied), its size, and where on the screen.
struct Picture {
    pixels: Vec<u8>,
    size: (u32, u32),
    at: (i32, i32),
}

static PICTURE: Mutex<Option<Picture>> = Mutex::new(None);
static INPUTS: Mutex<Vec<Input>> = Mutex::new(Vec::new());
static WINDOW: AtomicIsize = AtomicIsize::new(0);

const SHOW: u32 = WM_APP + 1;
const HIDE: u32 = WM_APP + 2;

/// Shows `pixels` (BGRA, premultiplied) of `size` with its top left at `at` on the screen.
pub(super) fn show(pixels: Vec<u8>, size: (u32, u32), at: (i32, i32)) {
    *PICTURE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Picture { pixels, size, at });
    post(SHOW);
}

pub(super) fn hide() {
    post(HIDE);
}

/// The mouse's doings since the last call.
pub(super) fn inputs() -> Vec<Input> {
    std::mem::take(&mut *INPUTS.lock().unwrap_or_else(|e| e.into_inner()))
}

fn post(message: u32) {
    let hwnd = window();
    if hwnd != 0 {
        // SAFETY: our window; a message without pointers.
        let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), message, WPARAM(0), LPARAM(0)) };
    }
}

/// The window, made (with its thread) the first time.
fn window() -> isize {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let (made, wait) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new().name("vilocity-menu".into()).spawn(move || run(made));
        let _ = wait.recv_timeout(std::time::Duration::from_secs(2));
    });
    WINDOW.load(Ordering::Acquire)
}

fn run(made: std::sync::mpsc::Sender<()>) {
    // SAFETY: a window class and a window of this thread, and its message loop.
    unsafe {
        let instance = GetModuleHandleW(None).map(|m| m.into()).unwrap_or_default();
        let class = WNDCLASSW {
            style: CS_HREDRAW,
            lpfnWndProc: Some(proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: w!("VilocityMenu"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("VilocityMenu"),
            w!("Vilocity settings"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance),
            None,
        );
        match hwnd {
            Ok(hwnd) => WINDOW.store(hwnd.0 as isize, Ordering::Release),
            Err(e) => tracing::warn!(error = %e, "the in-game menu's window could not be made"),
        }
        let _ = made.send(());
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let push = |input: Input| INPUTS.lock().unwrap_or_else(|e| e.into_inner()).push(input);
    let point = || ((lparam.0 & 0xFFFF) as i16 as i32, ((lparam.0 >> 16) & 0xFFFF) as i16 as i32);
    match message {
        SHOW => {
            if let Some(picture) = PICTURE.lock().unwrap_or_else(|e| e.into_inner()).take() {
                // SAFETY: our window, on its thread.
                unsafe { present(hwnd, &picture) };
            }
            LRESULT(0)
        }
        HIDE => {
            // SAFETY: as above.
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
            LRESULT(0)
        }
        // Clicked, it stays inactive: the game's window keeps the keyboard.
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            let (x, y) = point();
            push(Input::Click { x, y, right: message == WM_RBUTTONDOWN });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            push(Input::Wheel(((wparam.0 >> 16) & 0xFFFF) as i16 as i32 / 120));
            LRESULT(0)
        }
        WM_SETCURSOR if (lparam.0 & 0xFFFF) as u32 == HTCLIENT => {
            // SAFETY: the system's arrow.
            unsafe { SetCursor(LoadCursorW(None, IDC_ARROW).ok()) };
            LRESULT(1)
        }
        // SAFETY: the default for the rest.
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Puts `picture` on the window, at its place, and shows it (not activated).
///
/// SAFETY: `hwnd` is our layered window, on its thread.
unsafe fn present(hwnd: HWND, picture: &Picture) {
    let (w, h) = picture.size;
    if w == 0 || h == 0 || picture.pixels.len() < (w * h * 4) as usize {
        return;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: a DIB of the picture's size, filled, handed to the window, and let go.
    unsafe {
        let dc = CreateCompatibleDC(None);
        let mut bits = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            return;
        };
        std::ptr::copy_nonoverlapping(picture.pixels.as_ptr(), bits.cast::<u8>(), (w * h * 4) as usize);
        let old = SelectObject(dc, bitmap.into());
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let at = POINT { x: picture.at.0, y: picture.at.1 };
        let size = SIZE { cx: w as i32, cy: h as i32 };
        let from = POINT { x: 0, y: 0 };
        let _ = UpdateLayeredWindow(hwnd, None, Some(&at), Some(&size), Some(dc), Some(&from), COLORREF(0), Some(&blend), ULW_ALPHA);
        SelectObject(dc, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        // Shown above the other topmost windows (the frame rate panel's), not activated.
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
}
