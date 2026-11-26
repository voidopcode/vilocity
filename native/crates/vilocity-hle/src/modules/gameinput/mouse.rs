//! The mouse as part of the stand-in pad: its buttons press pad controls
//! ([`super::binds`]) and its motion pushes the right
//! stick. A left click inside the title's window takes the mouse (cursor
//! hidden and kept inside); left Alt, Esc, another window or a connected pad
//! gives it back.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Virtual keys of the mouse's buttons.
pub(super) const LEFT: u8 = 0x01;
pub(super) const RIGHT: u8 = 0x02;
pub(super) const MIDDLE: u8 = 0x04;
pub(super) const BACK: u8 = 0x05;
pub(super) const FORWARD: u8 = 0x06;
/// Left Alt gives the mouse back.
pub(super) const RELEASE: u8 = 0xA4;
/// So does Esc (Vilocity: it also opens the title's menu, where the cursor is wanted).
pub(super) const ESCAPE: u8 = 0x1B;

/// What the mouse does to the stand-in pad; nothing while it is not taken.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Mouse {
    /// Buttons held, as virtual keys.
    pub buttons: Vec<u8>,
    /// The right stick, up positive.
    pub stick: (f32, f32),
}

/// The title's window has the mouse.
pub(crate) static TAKEN: AtomicBool = AtomicBool::new(false);

/// The title's window has the mouse, so it shows no cursor.
pub(crate) fn taken() -> bool {
    TAKEN.load(Ordering::Relaxed)
}

/// Mouse counts per second that push the stick all the way, at sensitivity 1.
const FULL_SPEED: f32 = 2000.0;
/// Titles ignore a stick this close to its centre (XInput's value for the
/// right stick), so the slowest motion starts here.
const DEAD_ZONE: f32 = 8689.0 / 32767.0;

/// Mouse motion as a stick. A mouse reports distance and a stick speed, and
/// the title looks only once per frame: the stick is the mean speed over the
/// time between two looks, so a frame turns by all the motion made since the
/// one before, however long it took.
pub(super) struct Look {
    /// When (microseconds), and how far.
    moves: VecDeque<(u64, i32, i32)>,
    sensitivity: f32,
}

impl Look {
    pub fn new(sensitivity: f32) -> Look {
        Look {
            moves: VecDeque::new(),
            sensitivity,
        }
    }

    pub fn clear(&mut self) {
        self.moves.clear();
    }

    /// Adds motion (down positive, as mice report it); the stick now.
    pub fn stick(&mut self, now: u64, window: Duration, (dx, dy): (i32, i32)) -> (f32, f32) {
        let window = (window.as_micros() as u64).max(1);
        if (dx, dy) != (0, 0) {
            self.moves.push_back((now, dx, dy));
        }
        while self.moves.front().is_some_and(|m| m.0 + window <= now) {
            self.moves.pop_front();
        }
        let (x, y) = self
            .moves
            .iter()
            .fold((0.0, 0.0), |(x, y), m| (x + m.1 as f32, y - m.2 as f32));
        let length = x.hypot(y);
        if length == 0.0 {
            return (0.0, 0.0);
        }
        let speed = length * 1e6 / window as f32 * self.sensitivity / FULL_SPEED;
        let push = DEAD_ZONE + (1.0 - DEAD_ZONE) * speed.min(1.0);
        (x / length * push, y / length * push)
    }
}

/// How often the title looks at the stand-in pad.
#[derive(Default)]
pub(super) struct Pace {
    last: Option<u64>,
    interval: u64,
}

impl Pace {
    /// Several looks this close together belong to one frame.
    const SAME_FRAME: u64 = 8_000;
    const SHORTEST: u64 = 16_000;
    const LONGEST: u64 = 500_000;

    pub fn look(&mut self, now: u64) {
        let Some(last) = self.last else {
            self.last = Some(now);
            return;
        };
        let gap = now.saturating_sub(last);
        if gap < Self::SAME_FRAME {
            return;
        }
        self.last = Some(now);
        let gap = gap.min(Self::LONGEST);
        self.interval = if self.interval == 0 {
            gap
        } else {
            (self.interval * 3 + gap) / 4
        };
    }

    pub fn interval(&self) -> Duration {
        Duration::from_micros(self.interval.clamp(Self::SHORTEST, Self::LONGEST))
    }
}

/// Who has the mouse.
#[derive(Default)]
pub(super) struct Capture {
    taken: bool,
    left_was_down: bool,
    /// The click that took the mouse presses nothing.
    swallow: bool,
}

impl Capture {
    /// One poll. `title`: the title's window is in front and no pad is
    /// connected; `inside`: the cursor is in its client area. The buttons
    /// that count, `None` while the mouse is not taken.
    pub fn step(
        &mut self,
        title: bool,
        inside: bool,
        release: bool,
        mut held: Vec<u8>,
    ) -> Option<Vec<u8>> {
        let left = held.contains(&LEFT);
        let clicked = left && !self.left_was_down;
        self.left_was_down = left;
        self.swallow &= left;
        if !title || release {
            self.taken = false;
        } else if !self.taken && clicked && inside {
            self.taken = true;
            self.swallow = true;
        }
        if !self.taken {
            return None;
        }
        if self.swallow {
            held.retain(|&vk| vk != LEFT);
        }
        Some(held)
    }
}

#[cfg(windows)]
pub(super) use host::Host;

#[cfg(windows)]
mod host {
    use std::sync::Once;
    use std::sync::atomic::AtomicI32;

    use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::Input::{
        GetRawInputData, MOUSE_MOVE_ABSOLUTE, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
        RIDEV_INPUTSINK, RIM_TYPEMOUSE, RegisterRawInputDevices,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging as wm;
    use windows_sys::core::w;

    use super::*;
    use crate::modules::lifecycle;

    /// Motion since the last poll.
    static DX: AtomicI32 = AtomicI32::new(0);
    static DY: AtomicI32 = AtomicI32::new(0);

    pub(in super::super) struct Host {
        look: Look,
        capture: Capture,
    }

    impl Default for Host {
        fn default() -> Host {
            let sensitivity = std::env::var("VILOCITY_MOUSE_SENSITIVITY")
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .filter(|s: &f32| s.is_finite() && *s > 0.0)
                .unwrap_or(1.0);
            Host {
                look: Look::new(sensitivity),
                capture: Capture::default(),
            }
        }
    }

    impl Host {
        /// `wanted`: no pad is connected.
        pub fn poll(&mut self, now: u64, wanted: bool, window: Duration) -> Mouse {
            // SAFETY: plain queries.
            let down = |vk: u8| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
            // SAFETY: a plain query.
            let front = unsafe { wm::GetForegroundWindow() };
            let area = (wanted && lifecycle::is_title_window(front))
                .then(|| client_area(front))
                .flatten();
            let inside = area.as_ref().is_some_and(cursor_in);
            let held = [LEFT, RIGHT, MIDDLE, BACK, FORWARD]
                .into_iter()
                .filter(|&vk| down(vk))
                .collect();
            let buttons = self
                .capture
                .step(area.is_some(), inside, down(RELEASE) || down(ESCAPE), held);
            let was = taken();
            if buttons.is_some() != was {
                TAKEN.store(!was, Ordering::Relaxed);
                tracing::info!(taken = !was, "mouse");
                self.look.clear();
                DX.store(0, Ordering::Relaxed);
                DY.store(0, Ordering::Relaxed);
                if was {
                    // SAFETY: lets the cursor go anywhere again.
                    unsafe { wm::ClipCursor(std::ptr::null()) };
                }
            }
            let (Some(buttons), Some(area)) = (buttons, area) else {
                return Mouse::default();
            };
            keep_inside(&area);
            listen();
            let moved = (DX.swap(0, Ordering::Relaxed), DY.swap(0, Ordering::Relaxed));
            Mouse {
                buttons,
                stick: self.look.stick(now, window, moved),
            }
        }
    }

    /// The window's client area on the screen; `None` when it has none
    /// (minimised).
    fn client_area(hwnd: HWND) -> Option<RECT> {
        let mut area = RECT::default();
        let mut corner = POINT::default();
        // SAFETY: our own rectangle and point; a window that is gone fails.
        let known = unsafe {
            wm::GetClientRect(hwnd, &mut area) != 0 && ClientToScreen(hwnd, &mut corner) != 0
        };
        (known && area.right > 0 && area.bottom > 0).then(|| RECT {
            left: corner.x,
            top: corner.y,
            right: corner.x + area.right,
            bottom: corner.y + area.bottom,
        })
    }

    fn cursor_in(area: &RECT) -> bool {
        let mut at = POINT::default();
        // SAFETY: our own point.
        let known = unsafe { wm::GetCursorPos(&mut at) } != 0;
        known && (area.left..area.right).contains(&at.x) && (area.top..area.bottom).contains(&at.y)
    }

    /// The system drops the clip on its own (focus changes), and the window
    /// can move: checked every poll, set when it differs.
    fn keep_inside(area: &RECT) {
        let mut clip = RECT::default();
        // SAFETY: our own rectangles.
        unsafe {
            let known = wm::GetClipCursor(&mut clip) != 0;
            let sides = |r: &RECT| (r.left, r.top, r.right, r.bottom);
            if !known || sides(&clip) != sides(area) {
                wm::ClipCursor(area);
            }
        }
    }

    /// Starts listening to the mouse's own motion (raw input: no
    /// acceleration, and it goes on at the edge of the clip).
    fn listen() {
        static STARTED: Once = Once::new();
        STARTED.call_once(|| {
            let thread = std::thread::Builder::new()
                .name("vilocity-mouse".into())
                .spawn(|| {
                    if open().is_null() {
                        tracing::warn!("no raw mouse input: the mouse does not turn the view");
                    } else {
                        pump();
                    }
                });
            if let Err(e) = thread {
                tracing::warn!(%e, "no mouse thread: the mouse does not turn the view");
            }
        });
    }

    /// A message-only window of this thread that gets raw mouse input, with
    /// or without focus; null if there is none to be had.
    fn open() -> HWND {
        // SAFETY: a window of a system class, and our own registration.
        unsafe {
            let null = std::ptr::null_mut();
            let hwnd = wm::CreateWindowExW(
                0,
                w!("STATIC"),
                w!("vilocity mouse"),
                0,
                0,
                0,
                0,
                0,
                wm::HWND_MESSAGE,
                null,
                null,
                std::ptr::null(),
            );
            let mouse = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 2,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            };
            if hwnd.is_null()
                || RegisterRawInputDevices(&mouse, 1, size_of::<RAWINPUTDEVICE>() as u32) == 0
            {
                return null;
            }
            hwnd
        }
    }

    /// Reads this thread's raw mouse input until the process ends.
    fn pump() {
        // SAFETY: the standard message loop.
        unsafe {
            let mut message = wm::MSG::default();
            while wm::GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                if message.message == wm::WM_INPUT {
                    moved(message.lParam);
                }
                wm::DispatchMessageW(&message);
            }
        }
    }

    fn moved(handle: isize) {
        let mut input = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        // SAFETY: room for one report and its size; the union is read as
        // what the header says it is.
        unsafe {
            let got = GetRawInputData(
                handle as _,
                RID_INPUT,
                (&raw mut input).cast(),
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            );
            if got == u32::MAX || input.header.dwType != RIM_TYPEMOUSE {
                return;
            }
            let mouse = input.data.mouse;
            // shortcut: pointers that report places (tablets, remote
            // desktops) press buttons but do not turn the view.
            if mouse.usFlags & MOUSE_MOVE_ABSOLUTE == 0 {
                DX.fetch_add(mouse.lLastX, Ordering::Relaxed);
                DY.fetch_add(mouse.lLastY, Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn raw_mouse_input_opens() {
        let hwnd = open();
        assert!(!hwnd.is_null());
        // SAFETY: this thread's window.
        unsafe { wm::DestroyWindow(hwnd) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(100);

    #[test]
    fn motion_pushes_the_stick_past_the_dead_zone_and_rests_when_it_stops() {
        let mut look = Look::new(1.0);
        assert_eq!(look.stick(0, FRAME, (0, 0)), (0.0, 0.0));
        // Right, as slow as it gets: just past the dead zone.
        let (x, y) = look.stick(1_000, FRAME, (1, 0));
        assert!(x > DEAD_ZONE && x < DEAD_ZONE + 0.01 && y == 0.0);
        // Mouse down is stick down.
        let (_, y) = look.stick(2_000, FRAME, (0, 50));
        assert!(y < -DEAD_ZONE);
        // Nothing in the window any more.
        assert_eq!(look.stick(200_000, FRAME, (0, 0)), (0.0, 0.0));
        // However fast, the stick stays on its circle.
        let (x, y) = look.stick(300_000, FRAME, (-9_000, -9_000));
        assert!((x.hypot(y) - 1.0).abs() < 1e-5 && x < 0.0 && y > 0.0);
    }

    #[test]
    fn all_motion_between_two_looks_counts() {
        // 100 counts in 100 ms are half of full speed, in one go or in ten.
        let half = DEAD_ZONE + (1.0 - DEAD_ZONE) * 0.5;
        let mut look = Look::new(1.0);
        assert!((look.stick(0, FRAME, (100, 0)).0 - half).abs() < 1e-5);
        let mut look = Look::new(1.0);
        let mut last = 0.0;
        for n in 0..10 {
            last = look.stick(n * 10_000, FRAME, (10, 0)).0;
        }
        assert!((last - half).abs() < 1e-5);
        // Sensitivity scales the speed.
        let mut fast = Look::new(2.0);
        assert!((fast.stick(0, FRAME, (100, 0)).0 - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_window_follows_the_title() {
        let mut pace = Pace::default();
        assert_eq!(pace.interval(), Duration::from_millis(16));
        // Two looks a frame, 200 ms a frame.
        for n in 0..4 {
            pace.look(n * 200_000);
            pace.look(n * 200_000 + 1_000);
        }
        assert_eq!(pace.interval(), Duration::from_millis(200));
        // A long stop (a loading screen) does not stretch it for good.
        pace.look(60_000_000);
        assert!(pace.interval() <= Duration::from_millis(500));
    }

    #[test]
    fn a_click_in_the_window_takes_the_mouse_and_alt_gives_it_back() {
        let mut c = Capture::default();
        // Not taken: buttons press nothing, a click outside takes nothing.
        assert_eq!(c.step(true, true, false, vec![RIGHT]), None);
        assert_eq!(c.step(true, false, false, vec![LEFT]), None);
        // Held into the window is no click in it.
        assert_eq!(c.step(true, true, false, vec![LEFT]), None);
        assert_eq!(c.step(true, true, false, vec![]), None);
        // The click that takes the mouse presses nothing itself.
        assert_eq!(c.step(true, true, false, vec![LEFT]), Some(vec![]));
        assert_eq!(
            c.step(true, true, false, vec![LEFT, RIGHT]),
            Some(vec![RIGHT])
        );
        assert_eq!(c.step(true, true, false, vec![]), Some(vec![]));
        assert_eq!(c.step(true, true, false, vec![LEFT]), Some(vec![LEFT]));
        assert_eq!(c.step(true, true, false, vec![]), Some(vec![]));
        // Alt, then a click takes it again.
        assert_eq!(c.step(true, true, true, vec![]), None);
        assert_eq!(c.step(true, true, false, vec![]), None);
        assert_eq!(c.step(true, true, false, vec![LEFT]), Some(vec![]));
        // Another window in front, or a pad connected.
        assert_eq!(c.step(false, true, false, vec![]), None);
        assert_eq!(c.step(true, true, false, vec![]), None);
    }
}
