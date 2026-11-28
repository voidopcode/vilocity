//! The in-game menu: F1 shows it, F1 or Esc hides it. Up and down pick a
//! setting, left and right change it; the change shows in the next frame
//! (`crate::live`) and is kept for the next start at once. Enter on the last
//! line puts every setting back to what the launcher gave. A setting marked
//! `*` (the upscaler, the picture, ...; frame generation when it was off at
//! the start) is kept now and used from the next start: a change of one shows
//! in yellow, with a line saying to restart the game. While the menu is shown the game
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
    BI_RGB, CLEARTYPE_QUALITY, ClientToScreen, DT_CENTER, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection,
    CreateFontW, CreateSolidBrush, DEFAULT_CHARSET, DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_END_ELLIPSIS, DT_LEFT,
    DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FillRect, GdiFlush, HBITMAP,
    HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::core::w;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::menu_window::{self, Input};
use super::{Host, HostList};
use crate::live::{self, Knob};

const F1: u8 = 0x70;
const ESC: u8 = 0x1B;
const UP: u8 = 0x26;
const DOWN: u8 = 0x28;
const LEFT: u8 = 0x25;
const RIGHT: u8 = 0x27;
const ENTER: u8 = 0x0D;
const TAB: u8 = 0x09;
const PAGE_UP: u8 = 0x21;
const PAGE_DOWN: u8 = 0x22;
const SHIFT: u8 = 0x10;
const KEYS: [u8; 10] = [F1, ESC, UP, DOWN, LEFT, RIGHT, ENTER, TAB, PAGE_UP, PAGE_DOWN];

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
    name: "Upscaler *",
    knob: Knob::Upscaler,
    choices: &[("DLSS Off", 0.0), ("DLSS Quality", 1.0), ("DLSS Balanced", 2.0), ("DLSS Performance", 3.0), ("DLSS Ultra Perf.", 4.0)],
    help: "NVIDIA DLSS (Off: the game's own upscaling). Applies at the next start",
};
/// (Vilocity) The other cards' upscaler: AMD FSR at the game's own size, or at a mode's
/// (the game's FidelityFX path at that scale, as for DLSS; `VILOCITY_FSR_MODE`).
const UPSCALER_OTHER: Item = Item {
    name: "Upscaler *",
    knob: Knob::Upscaler,
    choices: &[("FSR (game's)", 0.0), ("FSR Quality", 1.0), ("FSR Balanced", 2.0), ("FSR Performance", 3.0), ("FSR Ultra Perf.", 4.0)],
    help: "AMD FSR at the game's size or a mode's (faster lower). Next start",
};
const FRAME_GEN: Item = Item {
    name: "Frame generation",
    knob: Knob::FrameGen,
    choices: &[("Off", 0.0), ("2x", 2.0), ("3x", 3.0), ("4x", 4.0)],
    help: "Made-up frames between real ones: DLSS, RTX 40/50, Windows GPU scheduling",
};
/// (Vilocity) The other cards' frame generation: AMD FSR's (`crate::fsr_fg`), one made-up
/// frame between two, from the next start.
const FRAME_GEN_FSR: Item = Item {
    name: "Frame generation *",
    knob: Knob::FrameGen,
    choices: &[("Off", 0.0), ("2x (FSR)", 2.0)],
    help: "AMD FSR: a made-up frame between two (needs Get FSR). Next start",
};
const PICTURE: Item = Item {
    name: "Picture *",
    knob: Knob::Picture,
    choices: &[("1080p (Series S)", 0.0), ("1440p (Series X)", 1.0), ("4K (Series X)", 2.0)],
    help: "Series X: sharper, slower, needs about 14 GB of video memory. Next start",
};
const DLSS_MODEL: Item = Item {
    name: "DLSS model *",
    knob: Knob::DlssModel,
    choices: &[("Default", 0.0), ("J", 1.0), ("K", 2.0), ("L", 3.0), ("M", 4.0)],
    help: "Which of NVIDIA's DLSS models upscales. Applies at the next start",
};
const HAZE: Item = Item {
    name: "Heat haze *",
    knob: Knob::Haze,
    choices: &[("On", 0.0), ("Off", 1.0)],
    help: "The game's shimmer above hot ground and fire. Applies at the next start",
};
const BLUR: Item = Item {
    name: "Motion blur *",
    knob: Knob::Blur,
    choices: &[("Off", 0.0), ("On", 1.0)],
    help: "The game's blur when the camera or things move fast. Next start",
};
const TORN_FIX: Item = Item {
    name: "Torn-frame fix *",
    knob: Knob::TornFix,
    choices: &[("Off", 0.0), ("On (slower)", 1.0)],
    help: "No frames with a character drawn torn; much slower. Next start",
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
const SHADE: Item = Item {
    name: "Shade in creases",
    knob: Knob::Shade,
    choices: &[("Off", 0.0), ("0.5", 0.5), ("1.0", 1.0)],
    help: "The light stand-in's darkening of corners and folds (faces too)",
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

const MOUSE: Item = Item {
    name: "Mouse sensitivity",
    knob: Knob::Mouse,
    choices: &[("0.25", 0.25), ("0.5", 0.5), ("0.75", 0.75), ("1.0", 1.0), ("1.25", 1.25), ("1.5", 1.5), ("2.0", 2.0), ("2.5", 2.5), ("3.0", 3.0), ("4.0", 4.0)],
    help: "How fast the camera turns with the mouse",
};
const INVERT_Y: Item = Item {
    name: "Invert mouse up/down",
    knob: Knob::InvertY,
    choices: &[("Off", 0.0), ("On", 1.0)],
    help: "On: moving the mouse up looks down",
};
const SMOOTH: Item = Item {
    name: "Mouse smoothing",
    knob: Knob::Smooth,
    choices: &[("Off", 0.0), ("Low", 0.5), ("Medium", 1.0), ("High", 2.0)],
    help: "Spreads mouse motion over more frames: smoother, a little later",
};
const VOLUME: Item = Item {
    name: "Sound volume",
    knob: Knob::Volume,
    choices: &[("0", 0.0), ("10", 10.0), ("20", 20.0), ("30", 30.0), ("40", 40.0), ("50", 50.0), ("60", 60.0), ("70", 70.0), ("80", 80.0), ("90", 90.0), ("100", 100.0)],
    help: "The game's volume (Windows' mixer)",
};
const PANEL: Item = Item {
    name: "Frame rate panel",
    knob: Knob::Panel,
    choices: &[("Hidden", 0.0), ("Compact", 1.0), ("Normal", 2.0), ("Detailed", 3.0)],
    help: "How much the panel shows; F8 hides and shows it too",
};
const PANEL_SIDE: Item = Item {
    name: "Panel side",
    knob: Knob::PanelSide,
    choices: &[("Left", 0.0), ("Right", 1.0)],
    help: "Which top corner of the game the panel is in",
};

/// The settings of an NVIDIA card (DLSS, frame generation), and of the others
/// (the game's own AMD FSR, no frame generation).
static NVIDIA_ITEMS: [Item; 20] =
    [PICTURE, UPSCALER_NVIDIA, DLSS_MODEL, FRAME_GEN, CAP, LIGHT, SHADE, NIGHT, FOG, REFLECTIONS, HAZE, BLUR, STEADY, TORN_FIX, MOUSE, INVERT_Y, SMOOTH, VOLUME, PANEL, PANEL_SIDE];
static OTHER_ITEMS: [Item; 19] =
    [PICTURE, UPSCALER_OTHER, FRAME_GEN_FSR, CAP, LIGHT, SHADE, NIGHT, FOG, REFLECTIONS, HAZE, BLUR, STEADY, TORN_FIX, MOUSE, INVERT_Y, SMOOTH, VOLUME, PANEL, PANEL_SIDE];

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
        Knob::Upscaler if on("VILOCITY_DLSS") || on("VILOCITY_FSR_MODE") => {
            let mode = std::env::var(if on("VILOCITY_DLSS") { "VILOCITY_DLSS_MODE" } else { "VILOCITY_FSR_MODE" });
            match mode.as_deref().map(str::trim) {
                Ok("balanced") => 2.0,
                Ok("performance") => 3.0,
                Ok("ultraperformance") => 4.0,
                Ok("dlaa" | "native") => 5.0,
                _ => 1.0,
            }
        }
        Knob::Upscaler => 0.0,
        Knob::FrameGen if on("VILOCITY_FRAMEGEN") || on("VILOCITY_FSR_FRAMEGEN") => 2.0,
        Knob::FrameGen => 0.0,
        Knob::Picture => match (std::env::var("VILOCITY_CONSOLE").as_deref(), std::env::var("VILOCITY_DISPLAY_MODE").as_deref()) {
            (Ok("series-x"), Ok("2560x1440")) => 1.0,
            (Ok("series-x"), _) => 2.0,
            _ => 0.0,
        },
        Knob::DlssModel => match std::env::var("VILOCITY_DLSS_PRESET").map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Ok("j") => 1.0,
            Ok("k") => 2.0,
            Ok("l") => 3.0,
            Ok("m") => 4.0,
            _ => 0.0,
        },
        Knob::Haze => f32::from(u8::from(std::env::var("VILOCITY_LEAVE_OUT").is_ok_and(|v| v.contains("731a39171b568298")))),
        Knob::TornFix => f32::from(u8::from(std::env::var("VILOCITY_CARD_STEPS").is_ok_and(|v| v.trim() == "0"))),
        Knob::Blur => f32::from(u8::from(!std::env::var("VILOCITY_LEAVE_OUT").is_ok_and(|v| v.contains("51e19585b4753814")))),
        Knob::Shade => std::env::var("VILOCITY_AMBIENT_SHADE")
            .ok()
            .and_then(|v| v.split(',').next().and_then(|s| s.trim().parse::<f32>().ok()))
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(1.0),
        Knob::Mouse => crate::live::mouse().0,
        Knob::InvertY | Knob::Smooth => 0.0,
        Knob::Panel => 2.0,
        Knob::PanelSide => 0.0,
        Knob::Volume => 100.0,
    }
}

/// The value `knob` has now: the menu's, else the start's.
fn value(knob: Knob) -> f32 {
    live::get(knob).unwrap_or_else(|| at_start(knob))
}

/// Whether a change of `knob` waits for the next start.
fn next_start(knob: Knob) -> bool {
    match knob {
        Knob::Upscaler | Knob::Picture | Knob::DlssModel | Knob::Haze | Knob::TornFix | Knob::Blur => true,
        Knob::FrameGen => !crate::streamline::framegen_live(),
        _ => false,
    }
}

/// The choice nearest to `v`.
fn nearest(choices: &[(&str, f32)], v: f32) -> usize {
    (0..choices.len()).min_by(|&a, &b| (choices[a].1 - v).abs().total_cmp(&(choices[b].1 - v).abs())).unwrap_or(0)
}

/// (Vilocity) The menu's tabs, each a page: settings, or the keys (2026-11-28).
const TABS: [&str; 6] = ["Graphics", "Light", "Mouse", "Keyboard", "Panel", "Sound"];
const KEYBOARD: usize = 3;

/// The tab a setting is on.
fn tab_of(knob: Knob) -> usize {
    match knob {
        Knob::Light | Knob::Shade | Knob::Night | Knob::Fog | Knob::Reflections => 1,
        Knob::Mouse | Knob::InvertY | Knob::Smooth => 2,
        Knob::Panel | Knob::PanelSide => 4,
        Knob::Volume => 5,
        _ => 0,
    }
}

/// The settings of a tab, in the order of [`items`].
fn tab_items(tab: usize) -> Vec<&'static Item> {
    items().iter().filter(|i| tab_of(i.knob) == tab).collect()
}

/// What a row does: a setting's choices, a key to press anew, or an action (Enter, a click).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Choice,
    Key,
    Action,
}

/// A row: its name, what it shows, what it does, and whether its change waits for a restart.
#[derive(Clone, PartialEq, Debug)]
struct Row {
    name: String,
    value: String,
    kind: Kind,
    pending: bool,
}

/// How many rows the menu shows at once (a longer tab scrolls).
const ROWS: usize = 13;

/// All the rows of `tab`, each with its help; `waiting`: the keys row whose new key is awaited.
fn rows_of(tab: usize, waiting: Option<usize>) -> Vec<(Row, &'static str)> {
    if tab == KEYBOARD {
        let mut rows: Vec<(Row, &'static str)> = crate::keys_page()
            .map(|p| p.rows())
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, (name, key))| {
                let wait = waiting == Some(i);
                let row = Row { name, value: if wait { "press a key".to_owned() } else { key }, kind: Kind::Key, pending: false };
                (row, if wait { "Press the new key (Esc: keep the one it has)" } else { "Enter or a click, then the new key: kept in keys.txt at once" })
            })
            .collect();
        rows.push((
            Row { name: "GTA V's keys again".to_owned(), value: "Enter".to_owned(), kind: Kind::Action, pending: false },
            "Every key as Vilocity comes (your file kept as keys-before-reset.txt)",
        ));
        return rows;
    }
    let mut rows: Vec<(Row, &'static str)> = tab_items(tab)
        .into_iter()
        .map(|item| {
            let v = value(item.knob);
            let at = nearest(item.choices, v);
            let shown = if (item.choices[at].1 - v).abs() < 1e-4 { item.choices[at].0.to_owned() } else { format!("{v:.2}") };
            let pending = next_start(item.knob) && (v - at_start(item.knob)).abs() > 1e-4;
            (Row { name: item.name.to_owned(), value: shown, kind: Kind::Choice, pending }, item.help)
        })
        .collect();
    rows.push((
        Row { name: "Reset this tab".to_owned(), value: "Enter".to_owned(), kind: Kind::Action, pending: false },
        "This tab's settings back to what the launcher started the game with",
    ));
    rows
}

/// What the menu shows: the tab, a window of its rows (from `first` of `total`), the row
/// picked (in the window), the help, the frame rate.
#[derive(Clone, PartialEq, Debug)]
struct Model {
    tab: usize,
    rows: Vec<Row>,
    first: usize,
    total: usize,
    picked: usize,
    help: &'static str,
    fps: Option<(u32, u32)>,
}

fn model(tab: usize, picked: usize, waiting: bool, fps: Option<(f64, f64)>) -> Model {
    let all = rows_of(tab, waiting.then_some(picked));
    let total = all.len();
    let picked = picked.min(total.saturating_sub(1));
    let first = picked.saturating_sub(ROWS / 2).min(total.saturating_sub(ROWS));
    let help = all.get(picked).map_or("", |r| r.1);
    Model {
        tab,
        rows: all.into_iter().skip(first).take(ROWS).map(|r| r.0).collect(),
        first,
        total,
        picked: picked - first,
        help,
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
const WIDTH: f32 = 520.0;
const TABS_TOP: f32 = 58.0;
const TABS_BOTTOM: f32 = 92.0;
const TOP: f32 = 104.0;
const ROW: f32 = 32.0;
const FOOT: f32 = 90.0;
const RADIUS: f32 = 14.0;
/// Where a row's value starts (its arrows: left half back, right half on).
const VALUE: f32 = 300.0;
/// The close button, top right.
const CLOSE: [f32; 4] = [WIDTH - 40.0, 8.0, WIDTH - 12.0, 36.0];

fn height() -> f32 {
    TOP + ROW * ROWS as f32 + FOOT
}

/// What a point of the menu (at 1080 lines) is on.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    Close,
    Tab(usize),
    /// A row of the window, and whether on its value's left (back) or right (on) half.
    Row(usize, Option<bool>),
    Nothing,
}

fn hit(x: f32, y: f32) -> Hit {
    let inside = |r: [f32; 4]| x >= r[0] && x < r[2] && y >= r[1] && y < r[3];
    if inside(CLOSE) {
        return Hit::Close;
    }
    if (TABS_TOP..TABS_BOTTOM).contains(&y) && (12.0..WIDTH - 12.0).contains(&x) {
        let each = (WIDTH - 24.0) / TABS.len() as f32;
        return Hit::Tab((((x - 12.0) / each) as usize).min(TABS.len() - 1));
    }
    if (TOP..TOP + ROW * ROWS as f32).contains(&y) && (10.0..WIDTH - 10.0).contains(&x) {
        let side = (x >= VALUE).then(|| x >= (VALUE + WIDTH - 20.0) / 2.0);
        return Hit::Row(((y - TOP) / ROW) as usize, side);
    }
    Hit::Nothing
}

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
        let size = (px(WIDTH) as u32, px(height()) as u32);
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
            // ClearType: the menu is shown at the screen's own pixels, on its opaque background.
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
                    CLEARTYPE_QUALITY,
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

    /// The menu as BGRA rows, alpha premultiplied: clear outside its rounded corners.
    fn paint(&self, m: &Model) -> Vec<u8> {
        let (w, h) = (WIDTH, height());
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
        self.text(1, WHITE, "Settings", [220.0, 8.0, CLOSE[0] - 10.0, 30.0], DT_RIGHT);
        self.text(3, GREY, "F1 or Esc to close", [220.0, 30.0, CLOSE[0] - 10.0, 48.0], DT_RIGHT);
        self.fill(TRACK, CLOSE);
        self.text(1, WHITE, "\u{00D7}", CLOSE, DT_CENTER);
        // The tabs: the one shown lit, underlined in the brand's colour.
        let each = (w - 24.0) / TABS.len() as f32;
        for (i, name) in TABS.iter().enumerate() {
            let x = 12.0 + each * i as f32;
            if i == m.tab {
                self.fill(TRACK, [x + 2.0, TABS_TOP, x + each - 2.0, TABS_BOTTOM]);
                self.fill(BRAND, [x + 2.0, TABS_BOTTOM - 3.0, x + each - 2.0, TABS_BOTTOM]);
            }
            self.text(2, if i == m.tab { WHITE } else { GREY }, name, [x, TABS_TOP, x + each, TABS_BOTTOM], DT_CENTER);
        }
        for (i, row) in m.rows.iter().enumerate() {
            let y = TOP + ROW * i as f32;
            let picked = i == m.picked;
            if picked {
                self.fill(TRACK, [10.0, y, w - 14.0, y + ROW - 2.0]);
                self.fill(BRAND, [10.0, y, 13.0, y + ROW - 2.0]);
            }
            self.text(2, if picked { WHITE } else { GREY }, &row.name, [22.0, y, VALUE, y + ROW - 2.0], DT_LEFT);
            let star = if row.pending { "  *" } else { "" };
            let shown = match row.kind {
                Kind::Choice => format!("\u{2039}  {}  \u{203A}{star}", row.value),
                Kind::Key | Kind::Action => row.value.clone(),
            };
            // A change that waits for a restart in yellow, picked or not.
            self.text(2, if picked || row.pending { FAIR } else { WHITE }, &shown, [VALUE, y, w - 20.0, y + ROW - 2.0], DT_RIGHT);
        }
        // A longer tab: where its window of rows is.
        if m.total > ROWS {
            let (top, all) = (TOP, ROW * ROWS as f32);
            self.fill(TRACK, [w - 9.0, top, w - 6.0, top + all]);
            let from = top + all * m.first as f32 / m.total as f32;
            self.fill(GREY, [w - 9.0, from, w - 6.0, from + all * ROWS as f32 / m.total as f32]);
        }
        let y = TOP + ROW * ROWS as f32 - 2.0;
        self.fill(TRACK, [14.0, y, w - 14.0, y + 1.0]);
        self.text(3, GREY, m.help, [22.0, y + 6.0, w - 18.0, y + 30.0], DT_LEFT);
        self.text(3, GREY, "Tab: next tab    \u{2191}\u{2193} pick    \u{2190}\u{2192} change    or the mouse", [22.0, y + 32.0, w - 18.0, y + 56.0], DT_LEFT);
        let (colour, line) = if m.tab == KEYBOARD {
            (GREY, "Walking and driving keys switch by themselves on F (G: by hand)")
        } else if m.rows.iter().any(|r| r.pending) {
            (FAIR, "Restart the game to use the changes in yellow")
        } else {
            (GREY, "Auto-saved, shown in the launcher; * needs a restart")
        };
        self.text(3, colour, line, [22.0, y + 56.0, w - 18.0, y + 80.0], DT_LEFT);
        // SAFETY: GDI is done with the bitmap; it holds `size` BGRA pixels.
        let mut out = unsafe {
            let _ = GdiFlush();
            std::slice::from_raw_parts(self.bits, (self.size.0 * self.size.1 * 4) as usize).to_vec()
        };
        // Opaque inside the rounded corners, clear (and black: premultiplied) outside.
        let (pw, ph) = (self.size.0 as f32, self.size.1 as f32);
        let r = RADIUS * self.scale;
        for (i, pixel) in out.chunks_exact_mut(4).enumerate() {
            let (x, y) = ((i as u32 % self.size.0) as f32 + 0.5, (i as u32 / self.size.0) as f32 + 0.5);
            let (dx, dy) = ((r - x).max(x - (pw - r)).max(0.0), (r - y).max(y - (ph - r)).max(0.0));
            if dx > 0.0 && dy > 0.0 && dx.hypot(dy) > r {
                pixel.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                pixel[3] = 0xFF;
            }
        }
        out
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
    /// The tab shown, and the row picked on each.
    tab: usize,
    picked: [usize; TABS.len()],
    /// The keys tab awaits a new key for its row picked (`true` once every key was up).
    waiting: Option<bool>,
    /// Per key of [`KEYS`]: held since, and when it last acted.
    held: [Option<(Instant, Instant)>; KEYS.len()],
    /// Closed with this key, still held.
    closing: Option<u8>,
    /// Presents since the frame rate was last taken, and since when.
    presents: (u32, Instant),
    fps: Option<(f64, f64)>,
    painter: Option<Painter>,
    /// What the window shows, if it is current, and where (screen pixels).
    painted: Option<(Model, (i32, i32))>,
    window_shown: bool,
}

impl Menu {
    pub(super) fn new() -> Menu {
        let open = std::env::var("VILOCITY_MENU_OPEN").unwrap_or_default();
        Menu {
            // (`VILOCITY_MENU_OPEN=1`: shown from the start, for a look at it; `=keys`: on its keys tab.)
            shown: open == "1" || open == "keys",
            tab: if open == "keys" { KEYBOARD } else { 0 },
            picked: [0; TABS.len()],
            waiting: None,
            held: [None; KEYS.len()],
            closing: None,
            presents: (0, Instant::now()),
            fps: None,
            painter: None,
            painted: None,
            window_shown: false,
        }
    }

    /// How many rows the tab shown has.
    fn total(&self) -> usize {
        rows_of(self.tab, None).len()
    }

    /// A frame was presented: keys (and the menu window's clicks) are looked at.
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
        // The keys tab's new key: the first one down once all are up (F1 still closes, Esc
        // keeps the key as it was; Shift, Ctrl and Alt are taken as their left or right one;
        // the mouse's buttons count too).
        if self.shown
            && let Some(armed) = self.waiting
        {
            let any = (1..=0xFEu8).filter(|&vk| vk != F1 && vk != ESC && !(0x10..=0x12).contains(&vk)).find(|&vk| down(vk));
            if !armed {
                self.waiting = Some(any.is_none() && !down(ESC));
            } else if let Some(vk) = any {
                let row = self.picked[KEYBOARD];
                self.waiting = None;
                let done = crate::keys_page().is_some_and(|p| p.bind(row, vk));
                tracing::info!(row, vk = format_args!("{vk:#04x}"), done, "in-game menu: a key changed");
                // The key that was taken does not act on the menu.
                acted.retain(|&k| k != vk);
                if let Some(i) = KEYS.iter().position(|&k| k == vk) {
                    self.held[i] = Some((now, now));
                }
                // Nor does the click that was taken.
                menu_window::inputs();
            }
        }
        let shift = down(SHIFT);
        for key in acted {
            self.press(key, shift);
        }
        for input in menu_window::inputs() {
            self.input(input);
        }
        HOLDS_INPUT.store(self.shown || self.closing.is_some(), Ordering::Relaxed);
    }

    fn close(&mut self, key: Option<u8>) {
        self.shown = false;
        self.waiting = None;
        self.closing = key;
    }

    fn press(&mut self, key: u8, shift: bool) {
        let tabs = TABS.len();
        match key {
            F1 if !self.shown => self.shown = true,
            // Esc while a key is awaited keeps the key as it was.
            ESC if self.shown && self.waiting.is_some() => self.waiting = None,
            F1 | ESC if self.shown => self.close(Some(key)),
            _ if !self.shown || self.waiting.is_some() => {}
            TAB if shift => self.tab = (self.tab + tabs - 1) % tabs,
            TAB | PAGE_DOWN => self.tab = (self.tab + 1) % tabs,
            PAGE_UP => self.tab = (self.tab + tabs - 1) % tabs,
            UP | DOWN => {
                let total = self.total().max(1);
                let p = &mut self.picked[self.tab];
                *p = if key == UP { (*p + total - 1) % total } else { (*p + 1) % total };
            }
            LEFT | RIGHT => self.change(key == RIGHT),
            ENTER => self.activate(),
            _ => {}
        }
    }

    /// The row picked's setting one choice back or on.
    fn change(&mut self, on: bool) {
        let Some(item) = tab_items(self.tab).get(self.picked[self.tab]).copied() else { return };
        let at = nearest(item.choices, value(item.knob));
        let to = if on { (at + 1).min(item.choices.len() - 1) } else { at.saturating_sub(1) };
        if to != at || (item.choices[at].1 - value(item.knob)).abs() > 1e-4 {
            live::set(item.knob, Some(item.choices[to].1));
            tracing::info!(setting = item.name, value = item.choices[to].0, "in-game menu");
            if item.knob == Knob::Volume {
                crate::volume::set(item.choices[to].1 / 100.0);
            }
        }
    }

    /// Enter (or a click) on the row picked: a key awaited, or the tab's reset.
    fn activate(&mut self) {
        let picked = self.picked[self.tab];
        let rows = rows_of(self.tab, None);
        match rows.get(picked).map(|r| r.0.kind) {
            Some(Kind::Key) => self.waiting = Some(false),
            Some(Kind::Action) if self.tab == KEYBOARD => {
                let done = crate::keys_page().is_some_and(|p| p.reset());
                tracing::info!(done, "in-game menu: GTA V's keys again");
            }
            Some(Kind::Action) => {
                for item in tab_items(self.tab) {
                    live::set(item.knob, None);
                    if item.knob == Knob::Volume {
                        crate::volume::set(1.0);
                    }
                }
                tracing::info!(tab = TABS[self.tab], "in-game menu: the tab's settings as the launcher started them");
            }
            _ => {}
        }
    }

    /// A click or a wheel turn on the menu's window.
    fn input(&mut self, input: Input) {
        if !self.shown {
            return;
        }
        let scale = self.painter.as_ref().map_or(1.0, |p| p.scale);
        match input {
            Input::Wheel(notches) if self.waiting.is_none() => {
                let total = self.total().max(1);
                let p = &mut self.picked[self.tab];
                *p = (*p as i64 - i64::from(notches)).clamp(0, total as i64 - 1) as usize;
            }
            Input::Click { x, y, .. } if self.waiting.is_none() => match hit(x as f32 / scale, y as f32 / scale) {
                Hit::Close => self.close(None),
                Hit::Tab(tab) => self.tab = tab,
                Hit::Row(i, side) => {
                    let first = self.painted.as_ref().map_or(0, |(m, _)| m.first);
                    let row = first + i;
                    if row >= self.total() {
                        return;
                    }
                    self.picked[self.tab] = row;
                    let kind = rows_of(self.tab, None).get(row).map(|r| r.0.kind);
                    match (kind, side) {
                        (Some(Kind::Choice), Some(on)) => self.change(on),
                        (Some(Kind::Key | Kind::Action), _) => self.activate(),
                        _ => {}
                    }
                }
                Hit::Nothing => {}
            },
            _ => {}
        }
    }

    /// Shows the menu, if shown, in its own window over the game's (see `menu_window`), at the
    /// screen's own pixels; hides that window otherwise. (Called where the frame is recorded.)
    pub(super) fn draw(&mut self, _host: &Host, _list: &HostList, _back: &ID3D12Resource, _size: (u32, u32), _format: DXGI_FORMAT) {
        if !self.shown {
            if self.window_shown {
                menu_window::hide();
                self.window_shown = false;
                self.painted = None;
            }
            return;
        }
        // Only while the game is in front (Alt+Tab hides it, F1 still open behind).
        let mut process = 0;
        // SAFETY: plain queries; room for the answer.
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut process) };
        if process != std::process::id() {
            if self.window_shown {
                menu_window::hide();
                self.window_shown = false;
                self.painted = None;
            }
            return;
        }
        // The game's window: its client area on the screen.
        let Some(game) = super::title_window() else { return };
        let mut client = RECT::default();
        let mut origin = windows::Win32::Foundation::POINT::default();
        // SAFETY: plain queries of the game's window.
        let placed = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetClientRect(game, &mut client).is_ok() && ClientToScreen(game, &mut origin).as_bool()
        };
        if !placed || client.bottom <= 0 {
            return;
        }
        let scale = (client.bottom as f32 / 1080.0).clamp(0.75, 3.0);
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
        let m = model(self.tab, self.picked[self.tab], self.waiting.is_some(), self.fps);
        let at = (
            origin.x + (client.right - painter.size.0 as i32) / 2,
            origin.y + (client.bottom - painter.size.1 as i32) / 2,
        );
        if self.painted.as_ref() != Some(&(m.clone(), at)) {
            menu_window::show(painter.paint(&m), painter.size, at);
            self.painted = Some((m, at));
            self.window_shown = true;
        }
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
        let upscaler = |items: &[Item]| items.iter().find(|i| i.knob == Knob::Upscaler).map(|i| i.choices).unwrap();
        assert!(upscaler(&NVIDIA_ITEMS).iter().all(|c| c.0.starts_with("DLSS")));
        assert!(NVIDIA_ITEMS.iter().any(|i| i.knob == Knob::FrameGen));
        assert!(upscaler(&OTHER_ITEMS).iter().all(|c| c.0.contains("FSR")));
        // The others' frame generation is AMD FSR's, at the next start.
        assert!(OTHER_ITEMS.iter().filter(|i| i.knob == Knob::FrameGen).all(|i| i.choices.iter().any(|c| c.0.contains("FSR")) && i.name.ends_with(" *")));
        // Every choice's text fits its place on the row.
        for item in NVIDIA_ITEMS.iter().chain(&OTHER_ITEMS) {
            assert!(item.choices.iter().all(|c| c.0.len() <= 18), "{}", item.name);
            assert!(item.help.len() <= 76, "{}", item.help);
        }
        assert_eq!(NVIDIA_ITEMS.len(), OTHER_ITEMS.len() + 1);
        // `*` on exactly the settings that wait for a restart (frame generation
        // depends on the start; AMD's one-choice upscaler cannot be changed).
        for item in NVIDIA_ITEMS.iter().chain(&OTHER_ITEMS).filter(|i| i.knob != Knob::FrameGen && i.choices.len() > 1) {
            assert_eq!(item.name.ends_with(" *"), next_start(item.knob), "{}", item.name);
        }
        assert!(OTHER_ITEMS.iter().all(|i| i.knob != Knob::DlssModel));
    }

    #[test]
    fn every_setting_is_on_one_tab_and_each_tab_ends_with_its_reset() {
        let on_tabs: usize = (0..TABS.len()).filter(|&t| t != KEYBOARD).map(|t| tab_items(t).len()).sum();
        assert_eq!(on_tabs, items().len());
        for tab in (0..TABS.len()).filter(|&t| t != KEYBOARD) {
            let m = model(tab, 0, false, Some((25.3, 39.5)));
            assert_eq!(m.total, tab_items(tab).len() + 1);
            assert!(m.total <= ROWS, "{} scrolls", TABS[tab]);
            assert_eq!(m.rows.last().unwrap().kind, Kind::Action);
            assert_eq!(m.fps, Some((253, 395)));
            for (row, item) in m.rows.iter().zip(tab_items(tab)) {
                assert_eq!(row.name, item.name);
                assert!(item.choices.iter().any(|c| c.0 == row.value) || row.value.parse::<f32>().is_ok(), "{row:?}");
            }
        }
        // The mouse finds what is painted where.
        assert_eq!(hit(WIDTH - 20.0, 20.0), Hit::Close);
        assert_eq!(hit(14.0, 70.0), Hit::Tab(0));
        assert_eq!(hit(WIDTH - 14.0, 70.0), Hit::Tab(TABS.len() - 1));
        assert_eq!(hit(40.0, TOP + ROW * 2.5), Hit::Row(2, None));
        assert_eq!(hit(VALUE + 5.0, TOP + 1.0), Hit::Row(0, Some(false)));
        assert_eq!(hit(WIDTH - 30.0, TOP + 1.0), Hit::Row(0, Some(true)));
    }

    /// The picture is painted where GDI is (any Windows): of its size, the
    /// panel's background inside, clear in the corners, something lighter where the text is.
    #[test]
    fn the_menu_is_painted() {
        let Some(painter) = Painter::new(1.0) else {
            eprintln!("no GDI here; not run");
            return;
        };
        let m = model(0, 2, false, Some((25.3, 39.5)));
        let picture = painter.paint(&m);
        let (w, h) = painter.size;
        assert_eq!(picture.len(), (w * h * 4) as usize);
        let at = |x: u32, y: u32| &picture[((y * w + x) * 4) as usize..][..4];
        assert_eq!(at(w - 30, h - 4), [22, 16, 14, 0xFF], "the panel's background, BGRA");
        assert_eq!(at(0, 0), [0, 0, 0, 0], "a rounded corner is clear");
        let lit = picture.chunks_exact(4).filter(|p| p[0] > 120 && p[1] > 120 && p[2] > 120).count();
        assert!(lit > 500, "text was drawn: {lit} light pixels");
        // The bar on the left in the colour of 25 fps (fair: yellow).
        assert_eq!(at(1, h / 2), [21, 204, 250, 0xFF]);
    }
    /// `VILOCITY_MENU_DUMP=<folder>`: every tab painted to `menu-<tab>.bmp` there, for a look
    /// (`cargo test -- --ignored menu_pictures`).
    #[test]
    #[ignore]
    fn menu_pictures() {
        let (Some(folder), Some(painter)) = (std::env::var_os("VILOCITY_MENU_DUMP"), Painter::new(1.5)) else { return };
        for tab in 0..TABS.len() {
            let picture = painter.paint(&model(tab, 1, false, Some((41.0, 24.4))));
            let (w, h) = painter.size;
            let mut bmp = Vec::new();
            bmp.extend_from_slice(b"BM");
            bmp.extend_from_slice(&(54 + picture.len() as u32).to_le_bytes());
            bmp.extend_from_slice(&[0; 4]);
            bmp.extend_from_slice(&54u32.to_le_bytes());
            bmp.extend_from_slice(&40u32.to_le_bytes());
            bmp.extend_from_slice(&(w as i32).to_le_bytes());
            bmp.extend_from_slice(&(-(h as i32)).to_le_bytes());
            bmp.extend_from_slice(&1u16.to_le_bytes());
            bmp.extend_from_slice(&32u16.to_le_bytes());
            bmp.extend_from_slice(&[0; 24]);
            bmp.extend_from_slice(&picture);
            std::fs::write(std::path::Path::new(&folder).join(format!("menu-{}.bmp", TABS[tab])), bmp).unwrap();
        }
    }
}
