//! The in-game menu: F1 shows it, F1 or Esc hides it. Up and down pick a
//! setting, left and right change it, and the change shows in the next frame
//! (`crate::live`) and is kept for the next start at once. Enter on the last
//! line puts every setting back to what the launcher gave. While it is shown
//! the game gets no keys and no mouse, and the key that closed it not until
//! it is let go (Esc would open the game's own pause menu).
//!
//! (Vilocity: the idea and the look are trustno1-coder/VIctory-NVIDIA's
//! in-game settings menu; this one is drawn with the engine's own text box,
//! `host::overlay`, and changes nothing that needs a restart.)

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Graphics::Direct3D12::ID3D12Resource;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::overlay::{Layout, Place, TextBox};
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

const LAYOUT: Layout = Layout { columns: 46, lines: 13 };

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

const ITEMS: [Item; 6] = [
    Item {
        name: "LIGHT IN THE SHADE",
        knob: Knob::Light,
        choices: &[("OFF", 0.0), ("0.4", 0.4), ("0.6", 0.6), ("0.8", 0.8), ("1.0", 1.0), ("1.2", 1.2), ("1.5", 1.5)],
        help: "STAND-IN FOR THE RAYS: BRIGHTER SHADE",
    },
    Item {
        name: "BRIGHTER NIGHTS",
        knob: Knob::Night,
        choices: &[("OFF", 0.0), ("0.03", 0.03), ("0.05", 0.05), ("0.08", 0.08), ("0.12", 0.12)],
        help: "LEAST LIGHT AT NIGHT: SEE DARK STREETS",
    },
    Item {
        name: "FOG LIGHT CAP",
        knob: Knob::Fog,
        choices: &[("GAME", 0.0), ("1.0", 1.0), ("1.5", 1.5), ("2.0", 2.0), ("3.0", 3.0)],
        help: "LOWER: LESS HAZE INDOORS AND AT SUNSET",
    },
    Item {
        name: "REFLECTIONS STAND-IN",
        knob: Knob::Reflections,
        choices: &[("OFF", 0.0), ("ON", 1.0)],
        help: "PUDDLES AND GLASS LIT BY THE ROOM",
    },
    Item {
        name: "STEADIER PICTURE",
        knob: Knob::Steady,
        choices: &[("OFF", 0.0), ("ON", 1.0)],
        help: "FEWER TORN FRAMES - ABOUT 1.5 MS SLOWER",
    },
    Item {
        name: "FRAME RATE CAP",
        knob: Knob::Cap,
        choices: &[("NONE", 0.0), ("30", 30.0), ("40", 40.0), ("45", 45.0), ("50", 50.0), ("60", 60.0)],
        help: "EVEN FRAMES: PICK ONE YOU REACH",
    },
];

/// The value `knob` has now: the menu's, else what the launcher gave.
fn value(knob: Knob) -> f32 {
    if let Some(v) = live::get(knob) {
        return v;
    }
    let number = |name: &str| std::env::var(name).ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite() && *v > 0.0);
    match knob {
        Knob::Light => number("VILOCITY_AMBIENT").unwrap_or(0.0),
        Knob::Night => number("VILOCITY_AMBIENT_FLOOR").unwrap_or(0.0),
        Knob::Fog => number("VILOCITY_AMBIENT_AIR").unwrap_or(0.0),
        Knob::Reflections => f32::from(u8::from(std::env::var("VILOCITY_AMBIENT_SPECULAR").is_ok_and(|v| !v.trim().is_empty() && v.trim() != "0"))),
        Knob::Steady => f32::from(u8::from(number("VILOCITY_SUBMIT_DEPTH") == Some(1.0))),
        Knob::Cap => 0.0,
    }
}

/// The choice nearest to `v`.
fn nearest(choices: &[(&str, f32)], v: f32) -> usize {
    (0..choices.len()).min_by(|&a, &b| (choices[a].1 - v).abs().total_cmp(&(choices[b].1 - v).abs())).unwrap_or(0)
}

/// The text of the menu with `picked` (a line of [`ITEMS`], or the reset
/// line after them) and the frame rate.
fn text(picked: usize, fps: f64, ms: f64) -> Vec<String> {
    let mut lines = vec![
        "VILOCITY SETTINGS            F1 OR ESC CLOSES".to_owned(),
        "UP/DOWN PICK  LEFT/RIGHT CHANGE  SAVED AT ONCE".to_owned(),
        String::new(),
    ];
    for item in &ITEMS {
        let v = value(item.knob);
        let at = nearest(item.choices, v);
        let shown = if (item.choices[at].1 - v).abs() < 1e-4 { item.choices[at].0.to_owned() } else { format!("{v:.2}") };
        lines.push(format!("  {:<23}< {:^6} >", item.name, shown));
    }
    lines.push(format!("  {:<23}  {:^6}", "RESET TO LAUNCHER", "ENTER"));
    lines.push(String::new());
    lines.push(ITEMS.get(picked).map_or("EVERY SETTING AS THE LAUNCHER STARTED IT", |i| i.help).to_owned());
    lines.push(format!("FPS {fps:.1}   {ms:.1} MS A FRAME"));
    lines
}

/// The menu of a presenter.
pub(super) struct Menu {
    shown: bool,
    picked: usize,
    /// Per key of [`KEYS`]: held since, and when it last acted.
    held: [Option<(Instant, Instant)>; KEYS.len()],
    /// Closed with this key, still held.
    closing: Option<u8>,
    /// The last presents, for the frame rate.
    presents: std::collections::VecDeque<Instant>,
    text_box: TextBox,
}

impl Menu {
    pub(super) fn new() -> Menu {
        Menu {
            // (`VILOCITY_MENU_OPEN=1`: shown from the start, for a look at it.)
            shown: std::env::var_os("VILOCITY_MENU_OPEN").is_some_and(|v| v == "1"),
            picked: 0,
            held: [None; KEYS.len()],
            closing: None,
            presents: std::collections::VecDeque::new(),
            text_box: TextBox::new(LAYOUT),
        }
    }

    /// A frame was presented: keys are looked at.
    pub(super) fn frame(&mut self) {
        let now = Instant::now();
        if self.shown {
            self.presents.push_back(now);
            while self.presents.len() > 31 {
                self.presents.pop_front();
            }
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
        if self.shown {
            let span = match (self.presents.front(), self.presents.back()) {
                (Some(a), Some(b)) if self.presents.len() > 1 => (*b - *a).as_secs_f64() / (self.presents.len() - 1) as f64,
                _ => 0.0,
            };
            let (fps, ms) = if span > 0.0 { (1.0 / span, span * 1e3) } else { (0.0, 0.0) };
            self.text_box.set(text(self.picked, fps, ms), Some(3 + self.picked));
        }
    }

    fn press(&mut self, key: u8) {
        match key {
            F1 if !self.shown => {
                self.shown = true;
                self.presents.clear();
            }
            F1 | ESC if self.shown => {
                self.shown = false;
                self.closing = Some(key);
            }
            _ if !self.shown => {}
            UP => self.picked = (self.picked + ITEMS.len()) % (ITEMS.len() + 1),
            DOWN => self.picked = (self.picked + 1) % (ITEMS.len() + 1),
            LEFT | RIGHT => {
                let Some(item) = ITEMS.get(self.picked) else { return };
                let at = nearest(item.choices, value(item.knob));
                let to = if key == LEFT { at.saturating_sub(1) } else { (at + 1).min(item.choices.len() - 1) };
                if to != at || (item.choices[at].1 - value(item.knob)).abs() > 1e-4 {
                    live::set(item.knob, Some(item.choices[to].1));
                    tracing::info!(setting = item.name, value = item.choices[to].0, "in-game menu");
                }
            }
            ENTER if self.picked == ITEMS.len() => {
                for item in &ITEMS {
                    live::set(item.knob, None);
                }
                tracing::info!("in-game menu: every setting as the launcher started it");
            }
            _ => {}
        }
    }

    /// Records the menu onto `back` (see `overlay::TextBox::draw`), if shown.
    pub(super) fn draw(&mut self, host: &Host, list: &HostList, back: &ID3D12Resource, size: (u32, u32), format: DXGI_FORMAT) {
        if self.shown {
            self.text_box.draw(host, list, back, size, format, Place::Middle);
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
    fn every_line_fits_the_box() {
        for picked in 0..=ITEMS.len() {
            let lines = text(picked, 123.4, 8.1);
            assert!(lines.len() <= LAYOUT.lines as usize, "{} lines", lines.len());
            for line in &lines {
                assert!(line.len() <= LAYOUT.columns as usize, "too long: {line:?}");
                let drawn = |c: u8| super::super::overlay::glyph(c.to_ascii_uppercase()) != super::super::overlay::glyph(b'#');
                assert!(line.bytes().all(drawn), "a character without a picture: {line:?}");
            }
        }
        for item in &ITEMS {
            assert!(item.help.len() <= LAYOUT.columns as usize, "{}", item.help);
            assert!(item.choices.iter().all(|c| c.0.len() <= 6));
        }
    }

    #[test]
    fn the_nearest_choice_is_picked() {
        let choices = [("OFF", 0.0), ("0.4", 0.4), ("0.8", 0.8)];
        assert_eq!(nearest(&choices, 0.0), 0);
        assert_eq!(nearest(&choices, 0.7), 2);
        assert_eq!(nearest(&choices, 0.5), 1);
        assert_eq!(nearest(&choices, 9.0), 2);
    }
}
