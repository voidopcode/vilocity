//! The in-game menu: F1 shows it, F1 or Esc hides it. Up and down pick a
//! setting, left and right change it; the change shows in the next frame
//! (`crate::live`) and is kept for the next start at once. Enter on the last
//! line puts every setting back to what the launcher gave. A setting marked
//! `*` (the upscaler, frame generation when it was off at the start) is
//! kept now and used from the next start. While the menu is shown the game
//! gets no keys and no mouse, and the key that closed it not until it is let
//! go (Esc would open the game's own pause menu).
//!
//! Drawn as the frame-rate panel is (`scripts\hud-gta6.ps1`): its colours,
//! Segoe UI, the bar on the left in the colour of the frame rate, rounded
//! corners; painted by Windows' GDI into a picture that `host::overlay`
//! shows.
//!
//! (Vilocity: the idea is trustno1-coder/VIctory-NVIDIA's in-game settings
//! menu; this one changes nothing that needs a restart while the game runs.)

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection,
    CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_END_ELLIPSIS, DT_LEFT,
    DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FillRect, GdiFlush, HBITMAP,
    HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::core::w;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::overlay::{Overlay, Place};
use super::{Host, HostList};
use crate::live::{self, Knob};

const F1: u8 = 0x70;
const ESC: u8 = 0x1B;
const UP: u8 = 0x26;
const DOWN: u8 = 0x28;
const LEFT: u8 = 0x25;
const RIGHT: u8 = 0x27;
const ENTER: u8 = 0x0D;
const KEYS: [u8; 7] = [F1, ESC, UP, DOWN, LEFT, RIGHT, ENTER];

/// A held arrow key repeats after this, then every `REPEAT`.
const FIRST_REPEAT: Duration = Duration::from_millis(400);
const REPEAT: Duration = Duration::from_millis(120);
/// How often the frame rate in the menu changes.
const FPS_EVERY: Duration = Duration::from_millis(500);

/// Shown, or closed with a key still held: the game gets no input.
static HOLDS_INPUT: AtomicBool = AtomicBool::new(false);

/// Whether the game is to get no keys and no mouse now.
pub(crate) fn holds_input() -> bool {
    HOLDS_INPUT.load(Ordering::Relaxed)
}

/// A setting: its name, its choices (shown, value), what it does.
struct Item {
    name: &'static str,
    knob: Knob,
    choices: &'static [(&'static str, f32)],
    help: &'static str,
}

const UPSCALER_NVIDIA: Item = Item {
    name: "Upscaler",
    knob: Knob::Upscaler,
    choices: &[("DLSS Off", 0.0), ("DLSS Quality", 1.0), ("DLSS Balanced", 2.0), ("DLSS Performance", 3.0)],
    help: "NVIDIA DLSS (Off: the game's own upscaling). Applies at the next start",
};
const UPSCALER_OTHER: Item = Item {
    name: "Upscaler",
    knob: Knob::Upscaler,
    choices: &[("AMD FSR", 0.0)],
    help: "The game's own AMD FidelityFX upscaling",
};
const FRAME_GEN: Item = Item {
    name: "Frame generation",
    knob: Knob::FrameGen,
    choices: &[("Off", 0.0), ("2x", 2.0), ("3x", 3.0), ("4x", 4.0)],
    help: "Made-up frames between real ones: DLSS, RTX 40/50, Windows GPU scheduling",
};
const LIGHT: Item = Item {
    name: "Light in the shade",
    knob: Knob::Light,
    choices: &[("Off", 0.0), ("0.4", 0.4), ("0.6", 0.6), ("0.8", 0.8), ("1.0", 1.0), ("1.2", 1.2), ("1.5", 1.5)],
    help: "A stand-in for the game's ray-traced light: brighter shade",
};
const NIGHT: Item = Item {
    name: "Brighter nights",
    knob: Knob::Night,
    choices: &[("Off", 0.0), ("0.03", 0.03), ("0.05", 0.05), ("0.08", 0.08), ("0.12", 0.12)],
    help: "The least light at night, so dark streets can be seen",
};
const FOG: Item = Item {
    name: "Fog light cap",
    knob: Knob::Fog,
    choices: &[("Game", 0.0), ("1.0", 1.0), ("1.5", 1.5), ("2.0", 2.0), ("3.0", 3.0)],
    help: "Lower: less haze indoors and at sunset",
};
const REFLECTIONS: Item = Item {
    name: "Reflections stand-in",
    knob: Knob::Reflections,
    choices: &[("Off", 0.0), ("On", 1.0)],
    help: "Puddles and glass lit by the room around them, not the sky",
};
const STEADY: Item = Item {
    name: "Steadier picture",
    knob: Knob::Steady,
    choices: &[("Off", 0.0), ("On", 1.0)],
    help: "Fewer torn frames, about 1.5 ms slower",
};
const CAP: Item = Item {
    name: "Frame rate cap",
    knob: Knob::Cap,
    choices: &[("None", 0.0), ("30", 30.0), ("40", 40.0), ("45", 45.0), ("50", 50.0), ("60", 60.0)],
    help: "Even frames: pick one the game reaches",
};

/// The settings of an NVIDIA card (DLSS, frame generation), and of the others
/// (the game's own AMD FSR, no frame generation).
static NVIDIA_ITEMS: [Item; 8] = [UPSCALER_NVIDIA, FRAME_GEN, LIGHT, NIGHT, FOG, REFLECTIONS, STEADY, CAP];
static OTHER_ITEMS: [Item; 7] = [UPSCALER_OTHER, LIGHT, NIGHT, FOG, REFLECTIONS, STEADY, CAP];

/// The settings this PC's card has.
fn items() -> &'static [Item] {
    static NVIDIA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let nvidia = *NVIDIA.get_or_init(|| crate::host::host().is_some_and(|h| crate::indirect::vendor(&h.device) == Some(0x10DE)));
    if nvidia { &NVIDIA_ITEMS } else { &OTHER_ITEMS }
}

/// What `knob` was at the start, as the launcher (and the menu's kept
/// settings, which the play script applied) gave it.
fn at_start(knob: Knob) -> f32 {
    let number = |name: &str| std::env::var(name).ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite() && *v > 0.0);
    let on = |name: &str| std::env::var(name).is_ok_and(|v| !v.trim().is_empty() && v.trim() != "0");
    match knob {
        Knob::Light => number("VILOCITY_AMBIENT").unwrap_or(0.0),
        Knob::Night => number("VILOCITY_AMBIENT_FLOOR").unwrap_or(0.0),
        Knob::Fog => number("VILOCITY_AMBIENT_AIR").unwrap_or(0.0),
        Knob::Reflections => f32::from(u8::from(on("VILOCITY_AMBIENT_SPECULAR"))),
        Knob::Steady => f32::from(u8::from(number("VILOCITY_SUBMIT_DEPTH") == Some(1.0))),
        Knob::Cap => 0.0,
        Knob::Upscaler if on("VILOCITY_DLSS") => match std::env::var("VILOCITY_DLSS_MODE").as_deref().map(str::trim) {
            Ok("balanced") => 2.0,
            Ok("performance") => 3.0,
            _ => 1.0,
        },
        Knob::Upscaler => 0.0,
        Knob::FrameGen if on("VILOCITY_FRAMEGEN") => 2.0,
        Knob::FrameGen => 0.0,
    }
}

/// The value `knob` has now: the menu's, else the start's.
fn value(knob: Knob) -> f32 {
    live::get(knob).unwrap_or_else(|| at_start(knob))
}

/// Whether a change of `knob` waits for the next start.
fn next_start(knob: Knob) -> bool {
    match knob {
        Knob::Upscaler => true,
        Knob::FrameGen => !crate::streamline::framegen_live(),
        _ => false,
    }
}

/// The choice nearest to `v`.
fn nearest(choices: &[(&str, f32)], v: f32) -> usize {
    (0..choices.len()).min_by(|&a, &b| (choices[a].1 - v).abs().total_cmp(&(choices[b].1 - v).abs())).unwrap_or(0)
}

/// What the menu shows: its rows (name, value, waits for the next start),
/// the row picked (the reset row after the items), the help, the frame rate.
#[derive(Clone, PartialEq, Debug)]
struct Model {
    rows: Vec<(&'static str, String, bool)>,
    picked: usize,
    help: &'static str,
    fps: Option<(u32, u32)>,
}

fn model(picked: usize, fps: Option<(f64, f64)>) -> Model {
    let mut rows: Vec<(&'static str, String, bool)> = items()
        .iter()
        .map(|item| {
            let v = value(item.knob);
            let at = nearest(item.choices, v);
            let shown = if (item.choices[at].1 - v).abs() < 1e-4 { item.choices[at].0.to_owned() } else { format!("{v:.2}") };
            let pending = next_start(item.knob) && (v - at_start(item.knob)).abs() > 1e-4;
            (item.name, shown, pending)
        })
        .collect();
    rows.push(("Reset to the launcher's settings", "Enter".to_owned(), false));
    Model {
        rows,
        picked,
        help: items().get(picked).map_or("Every setting back to what the launcher started the game with", |i| i.help),
        // (Tenths: a picture painted anew only when what it shows changes.)
        fps: fps.map(|(f, ms)| ((f * 10.0).round() as u32, (ms * 10.0).round() as u32)),
    }
}

/// The frame-rate panel's colours (`scripts\hud-gta6.ps1`), as GDI takes them.
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}
const BACK: COLORREF = rgb(14, 16, 22);
const WHITE: COLORREF = rgb(236, 238, 244);
const GREY: COLORREF = rgb(138, 144, 160);
const GOOD: COLORREF = rgb(61, 220, 132);
const FAIR: COLORREF = rgb(250, 204, 21);
const BAD: COLORREF = rgb(248, 113, 113);
const BRAND: COLORREF = rgb(124, 92, 255);
const TRACK: COLORREF = rgb(40, 44, 56);

/// Sizes in pixels at 1080 lines.
const WIDTH: f32 = 460.0;
const TOP: f32 = 74.0;
const ROW: f32 = 30.0;
const FOOT: f32 = 66.0;
const RADIUS: f32 = 14.0;

/// GDI's things for painting the menu at one scale.
struct Painter {
    scale: f32,
    size: (u32, u32),
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    /// The frame rate's, the title's, a row's, a small one.
    fonts: [HFONT; 4],
}

// SAFETY: GDI objects of this process, used under the presenter's lock only.
unsafe impl Send for Painter {}

impl Painter {
    fn new(scale: f32) -> Option<Painter> {
        let px = |v: f32| (v * scale).round() as i32;
        let size = (px(WIDTH) as u32, px(TOP + ROW * (items().len() + 1) as f32 + FOOT) as u32);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size.0 as i32,
                // (Top-down.)
                biHeight: -(size.1 as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: plain creations; what is made is let go in `drop` (or here, if a later one fails).
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let mut bits = std::ptr::null_mut();
            let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, bitmap.into());
            let font = |height: f32, weight: i32| {
                CreateFontW(
                    -px(height),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    ANTIALIASED_QUALITY,
                    0,
                    w!("Segoe UI"),
                )
            };
            let fonts = [font(32.0, 700), font(16.0, 600), font(15.0, 400), font(12.5, 600)];
            let _ = SetBkMode(dc, TRANSPARENT);
            Some(Painter { scale, size, dc, bitmap, old, bits: bits.cast(), fonts })
        }
    }

    /// `text` in `font` and `colour`, in `rect` (x0, y0, x1, y1 at 1080 lines).
    fn text(&self, font: usize, colour: COLORREF, text: &str, rect: [f32; 4], align: DRAW_TEXT_FORMAT) {
        let s = self.scale;
        let mut rect = RECT {
            left: (rect[0] * s) as i32,
            top: (rect[1] * s) as i32,
            right: (rect[2] * s) as i32,
            bottom: (rect[3] * s) as i32,
        };
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let flags = DRAW_TEXT_FORMAT(align.0 | DT_SINGLELINE.0 | DT_VCENTER.0 | DT_NOPREFIX.0 | DT_END_ELLIPSIS.0);
        // SAFETY: our DC; the text and the rectangle live through the call.
        unsafe {
            SelectObject(self.dc, self.fonts[font].into());
            SetTextColor(self.dc, colour);
            DrawTextW(self.dc, &mut wide, &mut rect, flags);
        }
    }

    /// A filled rectangle (x0, y0, x1, y1 at 1080 lines).
    fn fill(&self, colour: COLORREF, rect: [f32; 4]) {
        let s = self.scale;
        let rect = RECT {
            left: (rect[0] * s).round() as i32,
            top: (rect[1] * s).round() as i32,
            right: (rect[2] * s).round() as i32,
            bottom: (rect[3] * s).round() as i32,
        };
        // SAFETY: our DC; the brush is let go right after.
        unsafe {
            let brush = CreateSolidBrush(colour);
            FillRect(self.dc, &rect, brush);
            let _ = DeleteObject(brush.into());
        }
    }

    /// The menu as BGRA rows.
    fn paint(&self, m: &Model) -> Vec<u8> {
        let (w, h) = (WIDTH, self.size.1 as f32 / self.scale);
        self.fill(BACK, [0.0, 0.0, w, h]);
        // The frame rate, in the colour of how it plays, as on the panel.
        let colour = match m.fps {
            None => GREY,
            Some((f, _)) if f >= 280 => GOOD,
            Some((f, _)) if f >= 200 => FAIR,
            Some(_) => BAD,
        };
        self.fill(colour, [0.0, 0.0, 4.0, h]);
        let (fps, ms) = m.fps.map_or(("-".to_owned(), "-".to_owned()), |(f, ms)| (format!("{}", (f + 5) / 10), format!("{:.1} ms", f64::from(ms) / 10.0)));
        self.text(0, colour, &fps, [18.0, 4.0, 120.0, 48.0], DT_LEFT);
        self.text(3, GREY, "fps", [18.0 + 19.0 * fps.len() as f32, 22.0, 140.0, 44.0], DT_LEFT);
        self.text(2, WHITE, &ms, [120.0, 12.0, 220.0, 34.0], DT_LEFT);
        self.text(1, WHITE, "Settings", [220.0, 8.0, w - 18.0, 30.0], DT_RIGHT);
        self.text(3, GREY, "F1 or Esc to close", [220.0, 30.0, w - 18.0, 48.0], DT_RIGHT);
        self.fill(TRACK, [14.0, TOP - 14.0, w - 14.0, TOP - 13.0]);
        for (i, (name, value, pending)) in m.rows.iter().enumerate() {
            let y = TOP + ROW * i as f32 - 6.0;
            let picked = i == m.picked;
            if picked {
                self.fill(TRACK, [10.0, y, w - 10.0, y + ROW - 2.0]);
                self.fill(BRAND, [10.0, y, 13.0, y + ROW - 2.0]);
            }
            self.text(2, if picked { WHITE } else { GREY }, name, [22.0, y, 230.0, y + ROW - 2.0], DT_LEFT);
            let star = if *pending { "  *" } else { "" };
            let shown = if i < items().len() { format!("\u{2039}  {value}  \u{203A}{star}") } else { value.clone() };
            self.text(2, if picked { FAIR } else { WHITE }, &shown, [230.0, y, w - 20.0, y + ROW - 2.0], DT_RIGHT);
        }
        let y = TOP + ROW * m.rows.len() as f32 - 2.0;
        self.fill(TRACK, [14.0, y, w - 14.0, y + 1.0]);
        self.text(3, GREY, m.help, [22.0, y + 6.0, w - 18.0, y + 30.0], DT_LEFT);
        self.text(
            3,
            GREY,
            "\u{2191}\u{2193} pick     \u{2190}\u{2192} change     saved at once     * next start",
            [22.0, y + 32.0, w - 18.0, y + 56.0],
            DT_LEFT,
        );
        // SAFETY: GDI is done with the bitmap; it holds `size` BGRA pixels.
        unsafe {
            let _ = GdiFlush();
            let mut out = std::slice::from_raw_parts(self.bits, (self.size.0 * self.size.1 * 4) as usize).to_vec();
            for pixel in out.chunks_exact_mut(4) {
                pixel[3] = 0xFF;
            }
            out
        }
    }
}

impl Drop for Painter {
    fn drop(&mut self) {
        // SAFETY: ours, made in `new`.
        unsafe {
            SelectObject(self.dc, self.old);
            for font in self.fonts {
                let _ = DeleteObject(font.into());
            }
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// The menu of a presenter.
pub(super) struct Menu {
    shown: bool,
    picked: usize,
    /// Per key of [`KEYS`]: held since, and when it last acted.
    held: [Option<(Instant, Instant)>; KEYS.len()],
    /// Closed with this key, still held.
    closing: Option<u8>,
    /// Presents since the frame rate was last taken, and since when.
    presents: (u32, Instant),
    fps: Option<(f64, f64)>,
    painter: Option<Painter>,
    /// What the picture shows, if it is current.
    painted: Option<Model>,
    overlay: Overlay,
}

impl Menu {
    pub(super) fn new() -> Menu {
        Menu {
            // (`VILOCITY_MENU_OPEN=1`: shown from the start, for a look at it.)
            shown: std::env::var_os("VILOCITY_MENU_OPEN").is_some_and(|v| v == "1"),
            picked: 0,
            held: [None; KEYS.len()],
            closing: None,
            presents: (0, Instant::now()),
            fps: None,
            painter: None,
            painted: None,
            overlay: Overlay::new(),
        }
    }

    /// A frame was presented: keys are looked at.
    pub(super) fn frame(&mut self) {
        let now = Instant::now();
        self.presents.0 += 1;
        let since = now - self.presents.1;
        if since >= FPS_EVERY {
            let per = since.as_secs_f64() / f64::from(self.presents.0);
            self.fps = Some((1.0 / per, per * 1e3));
            self.presents = (0, now);
        }
        // SAFETY: plain queries.
        let down = |vk: u8| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
        if let Some(key) = self.closing
            && !down(key)
        {
            self.closing = None;
        }
        let mut process = 0;
        // SAFETY: plain queries; room for the answer.
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut process) };
        let ours = process == std::process::id();
        let mut acted = Vec::new();
        for (i, &key) in KEYS.iter().enumerate() {
            let held = ours && down(key);
            match (held, self.held[i]) {
                (false, _) => self.held[i] = None,
                (true, None) => {
                    self.held[i] = Some((now, now));
                    acted.push(key);
                }
                // Arrows repeat while held.
                (true, Some((since, last))) if matches!(key, UP | DOWN | LEFT | RIGHT) => {
                    if now - since >= FIRST_REPEAT && now - last >= REPEAT {
                        self.held[i] = Some((since, now));
                        acted.push(key);
                    }
                }
                (true, Some(_)) => {}
            }
        }
        for key in acted {
            self.press(key);
        }
        HOLDS_INPUT.store(self.shown || self.closing.is_some(), Ordering::Relaxed);
    }

    fn press(&mut self, key: u8) {
        match key {
            F1 if !self.shown => self.shown = true,
            F1 | ESC if self.shown => {
                self.shown = false;
                self.closing = Some(key);
            }
            _ if !self.shown => {}
            UP => self.picked = (self.picked + items().len()) % (items().len() + 1),
            DOWN => self.picked = (self.picked + 1) % (items().len() + 1),
            LEFT | RIGHT => {
                let Some(item) = items().get(self.picked) else { return };
                let at = nearest(item.choices, value(item.knob));
                let to = if key == LEFT { at.saturating_sub(1) } else { (at + 1).min(item.choices.len() - 1) };
                if to != at || (item.choices[at].1 - value(item.knob)).abs() > 1e-4 {
                    live::set(item.knob, Some(item.choices[to].1));
                    tracing::info!(setting = item.name, value = item.choices[to].0, "in-game menu");
                }
            }
            ENTER if self.picked == items().len() => {
                for item in items() {
                    live::set(item.knob, None);
                }
                tracing::info!("in-game menu: every setting as the launcher started it");
            }
            _ => {}
        }
    }

    /// Records the menu onto `back` (see `overlay::Overlay::draw`), if shown.
    pub(super) fn draw(&mut self, host: &Host, list: &HostList, back: &ID3D12Resource, size: (u32, u32), format: DXGI_FORMAT) {
        if !self.shown {
            return;
        }
        let scale = (size.1 as f32 / 1080.0).clamp(0.75, 3.0);
        if self.painter.as_ref().is_none_or(|p| (p.scale - scale).abs() > 1e-3) {
            self.painter = None;
            self.painter = Painter::new(scale);
            self.painted = None;
            if self.painter.is_none() {
                static SAID: AtomicBool = AtomicBool::new(false);
                if !SAID.swap(true, Ordering::Relaxed) {
                    tracing::warn!("the in-game menu could not be painted (GDI): it is closed");
                }
                self.shown = false;
                return;
            }
        }
        let Some(painter) = &self.painter else { return };
        let m = model(self.picked, self.fps);
        if self.painted.as_ref() != Some(&m) {
            self.overlay.set(painter.size, painter.paint(&m), (RADIUS * scale) as u32);
            self.painted = Some(m);
        }
        self.overlay.draw(host, list, back, size, format, Place::Middle);
    }
}

/// Before a present: with a frame rate cap ([`Knob::Cap`]), waits until the
/// frame's turn. A frame that is late by more than one turn starts the count
/// anew, so that a slow stretch is not made up for with a burst.
pub(super) fn pace() {
    static NEXT: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
    let mut next = NEXT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(cap) = live::get(Knob::Cap).filter(|c| *c >= 1.0) else {
        *next = None;
        return;
    };
    let turn = Duration::from_secs_f64(1.0 / f64::from(cap));
    let now = Instant::now();
    let due = next.unwrap_or(now);
    if due > now {
        std::thread::sleep(due - now);
    }
    let after = Instant::now();
    *next = Some(if after > due + turn { after + turn } else { due + turn });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_choice_is_picked() {
        let choices = [("Off", 0.0), ("0.4", 0.4), ("0.8", 0.8)];
        assert_eq!(nearest(&choices, 0.0), 0);
        assert_eq!(nearest(&choices, 0.7), 2);
        assert_eq!(nearest(&choices, 0.5), 1);
        assert_eq!(nearest(&choices, 9.0), 2);
    }

    /// Each card's settings: DLSS and frame generation on NVIDIA's alone,
    /// the game's AMD FSR on the others', the rest on both.
    #[test]
    fn each_card_has_its_own_upscaler() {
        assert!(NVIDIA_ITEMS[0].choices.iter().all(|c| c.0.starts_with("DLSS")));
        assert!(NVIDIA_ITEMS.iter().any(|i| i.knob == Knob::FrameGen));
        assert!(OTHER_ITEMS[0].choices.iter().all(|c| c.0.contains("FSR")));
        assert!(OTHER_ITEMS.iter().all(|i| i.knob != Knob::FrameGen));
        assert_eq!(NVIDIA_ITEMS.len(), OTHER_ITEMS.len() + 1);
    }

    #[test]
    fn every_setting_has_a_row_and_the_reset_comes_last() {
        let m = model(items().len(), Some((25.3, 39.5)));
        assert_eq!(m.rows.len(), items().len() + 1);
        assert_eq!(m.rows.last().unwrap().1, "Enter");
        assert_eq!(m.fps, Some((253, 395)));
        for (row, item) in m.rows.iter().zip(items()) {
            assert_eq!(row.0, item.name);
            assert!(item.choices.iter().any(|c| c.0 == row.1) || row.1.parse::<f32>().is_ok(), "{row:?}");
        }
    }

    /// The picture is painted where GDI is (any Windows): of its size, the
    /// panel's background in a corner, something lighter where the text is.
    #[test]
    fn the_menu_is_painted() {
        let Some(painter) = Painter::new(1.0) else {
            eprintln!("no GDI here; not run");
            return;
        };
        let m = model(2, Some((25.3, 39.5)));
        let picture = painter.paint(&m);
        let (w, h) = painter.size;
        assert_eq!(picture.len(), (w * h * 4) as usize);
        let at = |x: u32, y: u32| &picture[((y * w + x) * 4) as usize..][..4];
        assert_eq!(at(w - 2, h - 2), [22, 16, 14, 0xFF], "the panel's background, BGRA");
        let lit = picture.chunks_exact(4).filter(|p| p[0] > 120 && p[1] > 120 && p[2] > 120).count();
        assert!(lit > 500, "text was drawn: {lit} light pixels");
        // The bar on the left in the colour of 25 fps (fair: yellow).
        assert_eq!(at(1, h / 2), [21, 204, 250, 0xFF]);
    }
}
