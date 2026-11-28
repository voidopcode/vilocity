//! The mouse as part of the stand-in pad: its buttons press pad controls
//! ([`super::binds`]) and its motion pushes the right
//! stick. A left click inside the title's window takes the mouse (cursor
//! hidden and kept inside); only another window in front (Alt+Tab, the Windows
//! key) gives it back, as in GTA V on the PC (Esc and left Alt gave it back
//! too, and Esc is the game's pause: the cursor showed and clicks went to
//! Windows, 2026-11-28).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// Virtual keys of the mouse's buttons.
pub(super) const LEFT: u8 = 0x01;
pub(super) const RIGHT: u8 = 0x02;
pub(super) const MIDDLE: u8 = 0x04;
pub(super) const BACK: u8 = 0x05;
pub(super) const FORWARD: u8 = 0x06;
/// Left Alt (with Tab, another window comes to the front).
pub(super) const RELEASE: u8 = 0xA4;

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

/// Motion a full stick could not turn by in its frame is kept for the frames
/// after, up to this long a turn at full speed (seconds); more is let go, so
/// the camera never runs on long after the mouse stops.
const MOST_KEPT: f32 = 0.25;

/// How the player wants the mouse (the in-game menu's, `vilocity_gpu::mouse_settings`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Feel {
    pub sensitivity: f32,
    pub invert_y: bool,
    /// 0: each frame shows the motion since the one before; 1 or more: spread
    /// over that many more frames (smoother, later).
    pub smoothing: f32,
}

impl Default for Feel {
    fn default() -> Feel {
        Feel { sensitivity: 1.0, invert_y: false, smoothing: 0.0 }
    }
}

/// Mouse motion as a stick. A mouse reports distance and a stick speed, and
/// the title looks once a frame: the stick it sees is the speed that turns it,
/// in its next frame, by all the motion since its last look, and what an
/// earlier frame could not show (a full stick turns no faster, or a frame
/// came early) is shown in the frames after. No motion is lost or counted
/// twice when frames take longer or shorter (2026-11-28: the mean over a
/// window of past frames lost motion in long frames, counted it twice in
/// short ones, and flicks past full speed were lost).
#[derive(Default)]
pub(super) struct Look {
    /// Motion (right, up) since the title's last frame, and what is kept.
    pending: (f32, f32),
    kept: (f32, f32),
    /// The speed shown (0 to 1 of full speed), and when the title's last
    /// frame looked (microseconds; 0: not yet).
    shown: f32,
    frame_at: u64,
}

impl Look {
    pub fn new() -> Look {
        Look::default()
    }

    pub fn clear(&mut self) {
        *self = Look::default();
    }

    /// Adds motion (down positive, as mice report it), with `frame_at` the
    /// title's last frame's look and `window` the time between its frames;
    /// the stick for its next look.
    pub fn stick(&mut self, frame_at: u64, window: Duration, (dx, dy): (i32, i32), feel: Feel) -> (f32, f32) {
        // Mouse counts a second per share of full stick.
        let full = FULL_SPEED / feel.sensitivity.max(0.05);
        if frame_at != self.frame_at {
            if self.frame_at != 0 {
                // What the title turned by in that frame; the rest is kept.
                let gap = frame_at.saturating_sub(self.frame_at).clamp(8_000, 100_000) as f32 / 1e6;
                let turned = self.shown * full * gap;
                let (x, y) = (self.kept.0 + self.pending.0, self.kept.1 + self.pending.1);
                let length = x.hypot(y);
                let left = (length - turned).clamp(0.0, MOST_KEPT * full);
                self.kept = if length > 0.0 { (x / length * left, y / length * left) } else { (0.0, 0.0) };
            }
            self.pending = (0.0, 0.0);
            self.frame_at = frame_at;
        }
        let up = if feel.invert_y { dy } else { -dy } as f32;
        self.pending = (self.pending.0 + dx as f32, self.pending.1 + up);
        // (A frame's motion and a quarter second more: while the title does not look, no more.)
        let most = (MOST_KEPT + window.as_secs_f32()) * full;
        let length = self.pending.0.hypot(self.pending.1);
        if length > most {
            self.pending = (self.pending.0 / length * most, self.pending.1 / length * most);
        }
        let (x, y) = (self.kept.0 + self.pending.0, self.kept.1 + self.pending.1);
        let length = x.hypot(y);
        if length == 0.0 {
            self.shown = 0.0;
            return (0.0, 0.0);
        }
        let frame = window.as_secs_f32().max(0.008) * (1.0 + feel.smoothing.max(0.0));
        self.shown = (length / frame / full).min(1.0);
        let push = DEAD_ZONE + (1.0 - DEAD_ZONE) * self.shown;
        (x / length * push, y / length * push)
    }
}

/// When the title's last frame looked at the stand-in pad (microseconds), for [`Look`].
pub(super) static FRAME_LOOK: AtomicU64 = AtomicU64::new(0);

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
    /// A longer gap is a hitch (a load), not the frame time: it is left out
    /// of the mean (one of 500 ms had stretched it for some 8 frames).
    const LONGEST: u64 = 150_000;

    pub fn look(&mut self, now: u64) {
        let Some(last) = self.last else {
            self.last = Some(now);
            FRAME_LOOK.store(now.max(1), Ordering::Relaxed);
            return;
        };
        let gap = now.saturating_sub(last);
        if gap < Self::SAME_FRAME {
            return;
        }
        self.last = Some(now);
        FRAME_LOOK.store(now.max(1), Ordering::Relaxed);
        if gap > Self::LONGEST {
            return;
        }
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
    /// Given back for the in-game menu (its window takes the mouse): taken again when it
    /// closes, without a click.
    lent: bool,
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
        // (Vilocity) The in-game menu (F1) uses the mouse in its own window.
        if title && vilocity_gpu::menu_holds_input() {
            self.lent |= self.taken;
            self.taken = false;
            return None;
        }
        if std::mem::take(&mut self.lent) && title {
            self.taken = true;
            // A button held from the menu presses nothing until it is let go.
            self.swallow = left;
        }
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

    /// The mouse as the player set it (the in-game menu's, else `VILOCITY_MOUSE_SENSITIVITY`).
    fn feel() -> Feel {
        let (sensitivity, invert_y, smoothing) = vilocity_gpu::mouse_settings();
        Feel { sensitivity, invert_y, smoothing }
    }

    /// Motion since the last poll.
    static DX: AtomicI32 = AtomicI32::new(0);
    static DY: AtomicI32 = AtomicI32::new(0);

    pub(in super::super) struct Host {
        look: Look,
        capture: Capture,
    }

    impl Default for Host {
        fn default() -> Host {
            Host {
                look: Look::new(),
                capture: Capture::default(),
            }
        }
    }

    impl Host {
        /// `wanted`: no pad is connected.
        pub fn poll(&mut self, _now: u64, wanted: bool, window: Duration) -> Mouse {
            // SAFETY: plain queries.
            let down = |vk: u8| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
            // SAFETY: a plain query.
            let front = unsafe { wm::GetForegroundWindow() };
            let area = (wanted && !crate::modules::gameinput::keys_only() && lifecycle::is_title_window(front))
                .then(|| client_area(front))
                .flatten();
            let inside = area.as_ref().is_some_and(cursor_in);
            let held = [LEFT, RIGHT, MIDDLE, BACK, FORWARD]
                .into_iter()
                .filter(|&vk| down(vk))
                .collect();
            let buttons = self
                .capture
                .step(area.is_some(), inside, false, held);
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
                stick: self.look.stick(FRAME_LOOK.load(Ordering::Relaxed), window, moved, feel()),
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
    const FEEL: Feel = Feel { sensitivity: 1.0, invert_y: false, smoothing: 0.0 };
    /// The title's frame looks, 100 ms apart.
    const T: u64 = 100_000;

    #[test]
    fn motion_pushes_the_stick_past_the_dead_zone_and_rests_when_it_stops() {
        let mut look = Look::new();
        assert_eq!(look.stick(T, FRAME, (0, 0), FEEL), (0.0, 0.0));
        // Right, as slow as it gets: just past the dead zone.
        let (x, y) = look.stick(T, FRAME, (1, 0), FEEL);
        assert!(x > DEAD_ZONE && x < DEAD_ZONE + 0.01 && y == 0.0);
        // The title's next frame turned by it: the stick rests.
        assert_eq!(look.stick(2 * T, FRAME, (0, 0), FEEL), (0.0, 0.0));
        // Mouse down is stick down; inverted, up.
        let (_, y) = Look::new().stick(T, FRAME, (0, 50), FEEL);
        assert!(y < -DEAD_ZONE);
        let (_, y) = Look::new().stick(T, FRAME, (0, 50), Feel { invert_y: true, ..FEEL });
        assert!(y > DEAD_ZONE);
        // However fast, the stick stays on its circle.
        let (x, y) = Look::new().stick(T, FRAME, (-9_000, -9_000), FEEL);
        assert!((x.hypot(y) - 1.0).abs() < 1e-5 && x < 0.0 && y > 0.0);
    }

    #[test]
    fn all_motion_between_two_looks_counts() {
        // 100 counts in a 100 ms frame are half of full speed, in one go or in ten.
        let half = DEAD_ZONE + (1.0 - DEAD_ZONE) * 0.5;
        assert!((Look::new().stick(T, FRAME, (100, 0), FEEL).0 - half).abs() < 1e-5);
        let mut look = Look::new();
        let mut last = 0.0;
        for _ in 0..10 {
            last = look.stick(T, FRAME, (10, 0), FEEL).0;
        }
        assert!((last - half).abs() < 1e-5);
        // Sensitivity scales the speed.
        assert!((Look::new().stick(T, FRAME, (100, 0), Feel { sensitivity: 2.0, ..FEEL }).0 - 1.0).abs() < 1e-5);
    }

    /// The share of full speed a stick shows (past the dead zone).
    fn speed((x, y): (f32, f32)) -> f32 {
        let push = x.hypot(y);
        if push == 0.0 { 0.0 } else { (push - DEAD_ZONE) / (1.0 - DEAD_ZONE) }
    }

    #[test]
    fn a_flick_past_full_speed_turns_all_its_way_in_the_frames_after() {
        // 600 counts in one frame: three frames at full speed (200 counts each).
        let mut look = Look::new();
        let mut turned = 0.0;
        let mut stick = look.stick(T, FRAME, (600, 0), FEEL);
        for n in 2..8 {
            turned += speed(stick) * FULL_SPEED * 0.1;
            stick = look.stick(n * T, FRAME, (0, 0), FEEL);
        }
        assert!((turned - 600.0).abs() < 1.0, "turned {turned}");
        assert_eq!(stick, (0.0, 0.0));
        // A frame that came early (50 ms) turned half: the rest shows in the next.
        let mut look = Look::new();
        look.stick(T, FRAME, (100, 0), FEEL);
        let next = look.stick(T + 50_000, FRAME, (0, 0), FEEL);
        assert!((speed(next) - 0.25).abs() < 1e-3, "{next:?}");
        // Far more than a quarter second at full speed is let go: the camera stops soon.
        let mut look = Look::new();
        let mut stick = look.stick(T, FRAME, (50_000, 0), FEEL);
        let mut frames = 0;
        while stick != (0.0, 0.0) {
            frames += 1;
            stick = look.stick((frames + 1) * T, FRAME, (0, 0), FEEL);
        }
        assert!(frames <= 4, "{frames} frames");
    }

    #[test]
    fn the_window_follows_the_title() {
        let mut pace = Pace::default();
        assert_eq!(pace.interval(), Duration::from_millis(16));
        // Two looks a frame, 100 ms a frame.
        for n in 0..4 {
            pace.look(n * 100_000);
            pace.look(n * 100_000 + 1_000);
        }
        assert_eq!(pace.interval(), Duration::from_millis(100));
        // A long stop (a loading screen) is no frame: it leaves the mean as it was.
        pace.look(60_000_000);
        assert_eq!(pace.interval(), Duration::from_millis(100));
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
