//! Which keys press what on the stand-in pad: two layouts, one for walking
//! and one for driving, that the player can change in a text file
//! ([`use_file`]) while the title runs.
//!
//! A pad has the same controls in both and the title gives them other
//! meanings (a trigger fires on foot and is the accelerator in a car); which
//! of the two it is in, nothing tells us. So the player says it, with a key.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use super::{
    A, B, DOWN, LEFT, LEFT_SHOULDER, LEFT_STICK, MENU, RIGHT, RIGHT_SHOULDER, RIGHT_STICK, UP, VIEW, X, Y,
};

/// What a key does to the stand-in pad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Part {
    Button(u32),
    LeftTrigger,
    RightTrigger,
    /// The left stick, pushed this far (x, up positive y).
    Move(f32, f32),
    /// The right stick.
    Look(f32, f32),
}

/// One layout: keys (virtual keys, the mouse's buttons among them) and what
/// each does. A key may do several things and a thing may have several keys.
pub(super) type Layout = Vec<(u8, Part)>;

/// The names of the pad's controls in the file.
const PARTS: [(&str, Part); 24] = [
    ("A", Part::Button(A)),
    ("B", Part::Button(B)),
    ("X", Part::Button(X)),
    ("Y", Part::Button(Y)),
    ("LB", Part::Button(LEFT_SHOULDER)),
    ("RB", Part::Button(RIGHT_SHOULDER)),
    ("LT", Part::LeftTrigger),
    ("RT", Part::RightTrigger),
    ("L3", Part::Button(LEFT_STICK)),
    ("R3", Part::Button(RIGHT_STICK)),
    ("UP", Part::Button(UP)),
    ("DOWN", Part::Button(DOWN)),
    ("LEFT", Part::Button(LEFT)),
    ("RIGHT", Part::Button(RIGHT)),
    ("MENU", Part::Button(MENU)),
    ("VIEW", Part::Button(VIEW)),
    ("MOVE_UP", Part::Move(0.0, 1.0)),
    ("MOVE_DOWN", Part::Move(0.0, -1.0)),
    ("MOVE_LEFT", Part::Move(-1.0, 0.0)),
    ("MOVE_RIGHT", Part::Move(1.0, 0.0)),
    ("LOOK_UP", Part::Look(0.0, 1.0)),
    ("LOOK_DOWN", Part::Look(0.0, -1.0)),
    ("LOOK_LEFT", Part::Look(-1.0, 0.0)),
    ("LOOK_RIGHT", Part::Look(1.0, 0.0)),
];

/// The names of keys that are not a letter or a digit.
const KEYS: [(&str, u8); 66] = [
    ("LeftClick", 0x01),
    ("RightClick", 0x02),
    ("MiddleClick", 0x04),
    ("Mouse4", 0x05),
    ("Mouse5", 0x06),
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0D),
    ("Pause", 0x13),
    ("CapsLock", 0x14),
    ("Esc", 0x1B),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("ArrowLeft", 0x25),
    ("ArrowUp", 0x26),
    ("ArrowRight", 0x27),
    ("ArrowDown", 0x28),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Apps", 0x5D),
    ("Num0", 0x60),
    ("Num1", 0x61),
    ("Num2", 0x62),
    ("Num3", 0x63),
    ("Num4", 0x64),
    ("Num5", 0x65),
    ("Num6", 0x66),
    ("Num7", 0x67),
    ("Num8", 0x68),
    ("Num9", 0x69),
    ("NumMultiply", 0x6A),
    ("NumAdd", 0x6B),
    ("NumSubtract", 0x6D),
    ("NumDecimal", 0x6E),
    ("NumDivide", 0x6F),
    ("F1", 0x70),
    ("F2", 0x71),
    ("F3", 0x72),
    ("F4", 0x73),
    ("F5", 0x74),
    ("F6", 0x75),
    ("F7", 0x76),
    ("F8", 0x77),
    ("F9", 0x78),
    ("F10", 0x79),
    ("F12", 0x7B),
    ("ScrollLock", 0x91),
    ("Shift", 0xA0),
    ("RightShift", 0xA1),
    ("Ctrl", 0xA2),
    ("RightCtrl", 0xA3),
    ("RightAlt", 0xA5),
    ("Semicolon", 0xBA),
    ("Equals", 0xBB),
    ("Comma", 0xBC),
    ("Minus", 0xBD),
    ("Period", 0xBE),
    ("Slash", 0xBF),
    ("Backquote", 0xC0),
    ("LeftBracket", 0xDB),
    ("Backslash", 0xDC),
    ("RightBracket", 0xDD),
    ("Quote", 0xDE),
];

/// The file as it is made when there is none, and what holds without one.
pub(super) const DEFAULT: &str = "\
# Vilocity keys, as GTA V's on the PC, on GTA VI's own controls (its Settings,
# Control Scheme 'Default'). Change the keys on the right of '='; save; the
# running game takes them within a second. Several keys: separate them with
# commas. The in-game menu (F1) changes them too.
# Keys: letters, digits, F2, F4 to F7, F9, F10, F12, Shift, RightShift, Ctrl,
# RightCtrl, Space, Enter, Esc, Tab, CapsLock, Backspace, Insert, Delete, Home,
# End, PageUp, PageDown, Pause, ScrollLock, Apps, ArrowUp, ArrowDown, ArrowLeft,
# ArrowRight, Comma, Period, Slash, Semicolon, Quote, Minus, Equals, Backquote,
# LeftBracket, RightBracket, Backslash, Num0 to Num9, NumAdd, NumSubtract,
# NumMultiply, NumDivide, NumDecimal, LeftClick, RightClick, MiddleClick,
# Mouse4, Mouse5. The mouse's motion is always the right stick.
# Left out on purpose: F1 (Vilocity's settings), F3 and F8 (the frame rate
# panels), F11 (the whole screen and back), G (walking or driving keys, by
# hand), Right Alt (Windows presses Ctrl with it on AltGr keyboards), and the
# keys Windows keeps for itself (Windows, PrintScreen, NumLock).
# Delete this file to get the one Vilocity comes with again.
# (vilocity keys 2)

[general]
# Getting in or out (F): the driving keys hold from just after the get-in, the
# walking keys at once on the way out, as in GTA V.
ENTER_EXIT = F
# Switches between the two layouts by hand, if a get-in did not happen.
SWITCH = G

[walk]
# W A S D move, Shift runs, Space jumps, Ctrl crouches (held: rest).
MOVE_UP = W
MOVE_DOWN = S
MOVE_LEFT = A
MOVE_RIGHT = D
L3 = Shift, RightShift, Z
A = Space, Enter, Num0, 9
B = Ctrl, X, CapsLock, RightCtrl, Backspace, NumDecimal, 0
# Right click aims, left click shoots, R is melee, Q takes cover.
LT = RightClick, U, Num1
RT = LeftClick, O, Num3
X = R, B, Delete, Slash, 7, F9
RB = Q, Mouse5, Quote, 6, Num9, F7
# F or E: interact, get in. Tab: holster (held: weapons). C: look behind.
Y = F, E, Y, Insert, Backslash, 8, F10
LB = Tab, Mouse4, Semicolon, 5, Num7, F6
R3 = C, MiddleClick, H, Num5, F12
# Arrow up: phone. Down: minimap. Left: radio. Right: detonate.
UP = ArrowUp, 1, T, PageUp, NumAdd, F2
DOWN = ArrowDown, 2, N, PageDown, NumSubtract
LEFT = ArrowLeft, 3, LeftBracket, Minus, Comma, NumDivide, F4
RIGHT = ArrowRight, 4, RightBracket, Equals, Period, NumMultiply, F5
# Esc or P: pause. M or V: map.
MENU = Esc, P, Home, Pause, Apps
VIEW = M, V, Backquote, End, ScrollLock
LOOK_UP = I, Num8
LOOK_DOWN = K, Num2
LOOK_LEFT = J, Num4
LOOK_RIGHT = L, Num6

[drive]
# W accelerates, S brakes and reverses, A and D steer, Space is the handbrake.
RT = W, O, Num3
LT = S, U, Num1
MOVE_LEFT = A
MOVE_RIGHT = D
A = Space, Enter, Num0, 9
# Shift and Ctrl lean forward and back (bikes).
MOVE_UP = Shift, RightShift
MOVE_DOWN = Ctrl, RightCtrl
# E: horn (and nitrous). H: headlights (held: duck). F: get out. R: melee.
L3 = E, Z
B = H, X, CapsLock, Backspace, NumDecimal, 0
Y = F, Y, Insert, Backslash, 8, F10
X = R, B, Delete, Slash, 7, F9
# Left click shoots; right click or Tab: holster (held: weapons). C: look behind.
RB = LeftClick, Mouse5, Quote, 6, Num9, F7
LB = RightClick, Tab, Mouse4, Semicolon, 5, Num7, F6
R3 = C, MiddleClick, Num5, F12
# Q or arrow left: indicate left (held: the vehicle menu, radio and lights).
# Arrow right: indicate right. Arrow up: phone. Down: minimap.
LEFT = Q, ArrowLeft, 3, LeftBracket, Minus, Comma, NumDivide, F4
RIGHT = ArrowRight, 4, RightBracket, Equals, Period, NumMultiply, F5
UP = ArrowUp, 1, T, PageUp, NumAdd, F2
DOWN = ArrowDown, 2, N, PageDown, NumSubtract
MENU = Esc, P, Home, Pause, Apps
VIEW = M, V, Backquote, End, ScrollLock
LOOK_UP = I, Num8
LOOK_DOWN = K, Num2
LOOK_LEFT = J, Num4
LOOK_RIGHT = L, Num6
";

/// The virtual key of a name, whatever its case.
fn key(name: &str) -> Option<u8> {
    let name = name.trim();
    if let [c] = name.as_bytes()
        && c.is_ascii_alphanumeric()
    {
        return Some(c.to_ascii_uppercase());
    }
    KEYS.iter().find(|k| k.0.eq_ignore_ascii_case(name)).map(|k| k.1)
}

/// The two layouts, the key that switches them, and the key that gets in and
/// out (which switches them by itself).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Binds {
    pub walk: Layout,
    pub drive: Layout,
    pub switch: Vec<u8>,
    pub enter_exit: Vec<u8>,
}

impl Binds {
    /// Reads a file's text: what it says, and the lines it does not
    /// understand (number from 1, why). A section the file leaves out, or
    /// leaves empty, is the one Vilocity comes with.
    pub fn read(text: &str) -> (Binds, Vec<(usize, String)>) {
        let mut binds = Binds { walk: Vec::new(), drive: Vec::new(), switch: Vec::new(), enter_exit: Vec::new() };
        let mut wrong = Vec::new();
        let mut section = String::new();
        let mut named_switch = false;
        let mut named_enter = false;
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_ascii_lowercase();
                if !matches!(section.as_str(), "general" | "walk" | "drive") {
                    wrong.push((n + 1, format!("no section [{name}]")));
                }
                continue;
            }
            let Some((what, keys)) = line.split_once('=') else {
                wrong.push((n + 1, "no '='".into()));
                continue;
            };
            let mut found = Vec::new();
            for name in keys.split(',').filter(|k| !k.trim().is_empty()) {
                match key(name) {
                    Some(vk) => found.push(vk),
                    None => wrong.push((n + 1, format!("no key {:?}", name.trim()))),
                }
            }
            let what = what.trim().to_ascii_uppercase();
            let layout = match section.as_str() {
                "general" if what == "SWITCH" => {
                    named_switch = true;
                    binds.switch = found;
                    continue;
                }
                "general" if what == "ENTER_EXIT" => {
                    named_enter = true;
                    binds.enter_exit = found;
                    continue;
                }
                "walk" => &mut binds.walk,
                "drive" => &mut binds.drive,
                _ => {
                    wrong.push((n + 1, format!("{what} is not known here")));
                    continue;
                }
            };
            match PARTS.iter().find(|p| p.0 == what) {
                Some(&(_, part)) => layout.extend(found.into_iter().map(|vk| (vk, part))),
                None => wrong.push((n + 1, format!("no control {what}"))),
            }
        }
        if binds.walk.is_empty() || binds.drive.is_empty() || !named_switch || !named_enter {
            // (`DEFAULT` names all four: no endless reading.)
            let (own, _) = Binds::read(DEFAULT);
            if binds.walk.is_empty() {
                binds.walk = own.walk;
            }
            if binds.drive.is_empty() {
                binds.drive = own.drive;
            }
            if !named_switch {
                binds.switch = own.switch;
            }
            if !named_enter {
                binds.enter_exit = own.enter_exit;
            }
        }
        (binds, wrong)
    }
}

/// The player's file, the layouts it names and which of them holds now.
struct Now {
    file: Option<PathBuf>,
    /// When the file was looked at last, and its time of change then.
    looked: Option<Instant>,
    changed: Option<SystemTime>,
    binds: Option<Binds>,
    driving: bool,
    switch_was_down: bool,
    enter_was_down: bool,
    /// When the driving keys take over after a get-in.
    pending: Option<Instant>,
}

static NOW: Mutex<Now> = Mutex::new(Now {
    file: None,
    looked: None,
    changed: None,
    binds: None,
    driving: false,
    switch_was_down: false,
    enter_was_down: false,
    pending: None,
});

/// The get-in takes about this long; the driving keys hold after it (W
/// pressed while getting in still moves the player, not the car).
const GET_IN: Duration = Duration::from_millis(1500);

/// The file Vilocity made before its keys were GTA V's (2026-11-27): a
/// player's file that still says what it said is made the new one at the
/// start (the old one kept beside it as `keys-before-gta5.txt`).
const BEFORE: &str = include_str!("keys-2026-11-27.txt");

/// How often the file is looked at for changes.
const LOOK: Duration = Duration::from_secs(1);

/// The player's keys are in `file`, which is made if it is not there.
pub fn use_file(file: PathBuf) {
    if !file.exists()
        && let Err(e) = std::fs::write(&file, DEFAULT)
    {
        tracing::warn!(error = %e, file = %file.display(), "the keys file could not be made: the keys Vilocity comes with hold");
    }
    if let Ok(text) = std::fs::read_to_string(&file)
        && !text.contains("(vilocity keys 2)")
        && Binds::read(&text).0 == Binds::read(BEFORE).0
    {
        let kept = file.with_file_name("keys-before-gta5.txt");
        match std::fs::copy(&file, &kept).and_then(|_| std::fs::write(&file, DEFAULT)) {
            Ok(()) => tracing::info!(kept = %kept.display(), "keys file: Vilocity's keys are GTA V's now (the earlier ones kept)"),
            Err(e) => tracing::warn!(error = %e, "keys file: the earlier keys could not be made GTA V's"),
        }
    }
    tracing::info!(file = %file.display(), "keys of the stand-in pad: change them in this file");
    vilocity_gpu::set_keys_page(Box::new(Page));
    NOW.lock().unwrap_or_else(PoisonError::into_inner).file = Some(file);
}

impl Now {
    fn look(&mut self, now: Instant) {
        if self.binds.is_some() && self.looked.is_some_and(|at| now.duration_since(at) < LOOK) {
            return;
        }
        self.looked = Some(now);
        let changed = self.file.as_ref().and_then(|f| std::fs::metadata(f).ok()?.modified().ok());
        if self.binds.is_some() && changed == self.changed {
            return;
        }
        self.changed = changed;
        let text = self.file.as_ref().and_then(|f| std::fs::read_to_string(f).ok());
        let (binds, wrong) = Binds::read(text.as_deref().unwrap_or(DEFAULT));
        for (line, why) in &wrong {
            tracing::warn!(line, %why, "keys file: a line is not understood");
        }
        if self.binds.as_ref().is_some_and(|b| *b != binds) {
            tracing::info!("keys file: the new keys hold");
        }
        self.binds = Some(binds);
    }

    /// The layout that holds, with `down` saying which keys are.
    fn layout(&mut self, now: Instant, down: impl Fn(u8) -> bool) -> Layout {
        self.look(now);
        let Some(binds) = &self.binds else {
            return Vec::new();
        };
        let switch = binds.switch.iter().any(|&vk| down(vk));
        let enter = binds.enter_exit.iter().any(|&vk| down(vk));
        if switch && !self.switch_was_down {
            // By hand (G): the other layout at once, and no get-in waits.
            self.driving = !self.driving;
            self.pending = None;
            tracing::info!(layout = if self.driving { "drive" } else { "walk" }, how = "by hand", "keys");
        } else if enter && !self.enter_was_down {
            if self.driving {
                // Getting out: the walking keys at once.
                self.driving = false;
                self.pending = None;
                tracing::info!(layout = "walk", how = "getting out", "keys");
            } else {
                self.pending = Some(now + GET_IN);
            }
        }
        self.switch_was_down = switch;
        self.enter_was_down = enter;
        if self.pending.is_some_and(|at| now >= at) {
            self.pending = None;
            self.driving = true;
            tracing::info!(layout = "drive", how = "after a get-in", "keys");
        }
        if self.driving { binds.drive.clone() } else { binds.walk.clone() }
    }
}

/// The layout that holds now; a press of the switch key or of the get-in key,
/// seen here, changes it.
pub(super) fn layout(down: impl Fn(u8) -> bool) -> Layout {
    NOW.lock().unwrap_or_else(PoisonError::into_inner).layout(Instant::now(), down)
}

/// (Vilocity) The in-game menu's keys page (`vilocity_gpu::KeysPage`): a row per
/// control, by GTA VI's own names for it, in a section of the keys file ("both": the
/// walking and the driving one alike).
const ROWS: [(&str, &str, &str); 40] = [
    ("On foot: forward", "walk", "MOVE_UP"),
    ("On foot: back", "walk", "MOVE_DOWN"),
    ("On foot: left", "walk", "MOVE_LEFT"),
    ("On foot: right", "walk", "MOVE_RIGHT"),
    ("On foot: run", "walk", "L3"),
    ("On foot: jump, climb", "walk", "A"),
    ("On foot: crouch", "walk", "B"),
    ("On foot: melee", "walk", "X"),
    ("On foot: interact, get in", "walk", "Y"),
    ("On foot: cover", "walk", "RB"),
    ("On foot: aim", "walk", "LT"),
    ("On foot: shoot", "walk", "RT"),
    ("On foot: holster, weapons", "walk", "LB"),
    ("On foot: look behind", "walk", "R3"),
    ("On foot: phone", "walk", "UP"),
    ("On foot: minimap", "walk", "DOWN"),
    ("On foot: radio", "walk", "LEFT"),
    ("On foot: detonate", "walk", "RIGHT"),
    ("Pause", "both", "MENU"),
    ("Map", "both", "VIEW"),
    ("Car: accelerate", "drive", "RT"),
    ("Car: brake, reverse", "drive", "LT"),
    ("Car: steer left", "drive", "MOVE_LEFT"),
    ("Car: steer right", "drive", "MOVE_RIGHT"),
    ("Car: handbrake", "drive", "A"),
    ("Car: horn, nitrous", "drive", "L3"),
    ("Car: headlights, duck", "drive", "B"),
    ("Car: get out", "drive", "Y"),
    ("Car: shoot", "drive", "RB"),
    ("Car: holster, weapons", "drive", "LB"),
    ("Car: melee", "drive", "X"),
    ("Car: look behind", "drive", "R3"),
    ("Car: vehicle menu", "drive", "LEFT"),
    ("Car: indicate right", "drive", "RIGHT"),
    ("Car: phone", "drive", "UP"),
    ("Car: minimap", "drive", "DOWN"),
    ("Bike: lean forward", "drive", "MOVE_UP"),
    ("Bike: lean back", "drive", "MOVE_DOWN"),
    ("Get in and out", "general", "ENTER_EXIT"),
    ("Walk/drive keys by hand", "general", "SWITCH"),
];

/// The file's name of virtual key `vk`, if it has one.
fn name(vk: u8) -> Option<String> {
    if vk.is_ascii_uppercase() || vk.is_ascii_digit() {
        return Some(char::from(vk).to_string());
    }
    KEYS.iter().find(|k| k.1 == vk).map(|k| k.0.to_owned())
}

/// Whether a line's section is one of `section` ("both": walk and drive).
fn in_section(at: &str, section: &str) -> bool {
    at == section || (section == "both" && (at == "walk" || at == "drive"))
}

/// The keys of `control` in `section` of a file's text (the first section named, for "both").
fn keys_of_control(text: &str, section: &str, control: &str) -> Vec<String> {
    let mut at = String::new();
    for line in text.lines() {
        let data = line.split('#').next().unwrap_or("").trim();
        if let Some(name) = data.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            at = name.trim().to_ascii_lowercase();
            continue;
        }
        if let Some((what, keys)) = data.split_once('=')
            && in_section(&at, section)
            && what.trim().eq_ignore_ascii_case(control)
        {
            return keys.split(',').map(|k| k.trim().to_owned()).filter(|k| !k.is_empty()).collect();
        }
    }
    Vec::new()
}

/// `text` with key `new` (a name) as the first key of `control` in `section`, and taken
/// from the other controls of that section (one key, one thing); a line it lacks is added.
fn bound(text: &str, section: &str, control: &str, new: &str) -> String {
    let same = |a: &str| key(a).is_some() && key(a) == key(new);
    let mut at = String::new();
    let mut out = Vec::new();
    let mut found = std::collections::HashSet::new();
    let lines: Vec<&str> = text.lines().collect();
    for (n, line) in lines.iter().enumerate() {
        let (data, comment) = match line.find('#') {
            Some(i) => (&line[..i], &line[i..]),
            None => (*line, ""),
        };
        let trimmed = data.trim();
        if let Some(name) = trimmed.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            at = name.trim().to_ascii_lowercase();
            out.push(line.to_string());
            continue;
        }
        let Some((what, keys)) = trimmed.split_once('=').filter(|_| in_section(&at, section)) else {
            out.push(line.to_string());
            continue;
        };
        let what = what.trim();
        let mut keys: Vec<&str> = keys.split(',').map(str::trim).filter(|k| !k.is_empty() && !same(k)).collect();
        if what.eq_ignore_ascii_case(control) {
            // The new key in place of the first (the others stay as more keys for it).
            match keys.first_mut() {
                Some(first) => *first = new,
                None => keys.push(new),
            }
            found.insert(at.clone());
        }
        let gap = if comment.is_empty() { "" } else { " " };
        out.push(format!("{what} = {}{gap}{comment}", keys.join(", ")));
        // A section that ends without the control gets it.
        let next_is_section = lines.get(n + 1).is_none_or(|l| l.trim().starts_with('['));
        if next_is_section && !found.contains(&at) {
            out.push(format!("{control} = {new}"));
            found.insert(at.clone());
        }
    }
    out.join("\n") + "\n"
}

/// The keys page, on the player's keys file (made the new one at once).
struct Page;

impl vilocity_gpu::KeysPage for Page {
    fn rows(&self) -> Vec<(String, String)> {
        let file = NOW.lock().unwrap_or_else(PoisonError::into_inner).file.clone();
        let text = file.and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_else(|| DEFAULT.to_owned());
        ROWS.iter()
            .map(|&(what, section, control)| {
                let keys = keys_of_control(&text, if section == "both" { "walk" } else { section }, control);
                (what.to_owned(), keys.first().cloned().unwrap_or_else(|| "-".to_owned()))
            })
            .collect()
    }

    fn bind(&self, row: usize, vk: u8) -> bool {
        // (F3 and F8 are the frame rate panels', F1 the menu's.)
        let (Some(&(what, section, control)), Some(new)) = (ROWS.get(row), name(vk).filter(|_| !matches!(vk, 0x70 | 0x72 | 0x77)))
        else {
            return false;
        };
        let mut now = NOW.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(file) = now.file.clone() else { return false };
        let text = std::fs::read_to_string(&file).unwrap_or_else(|_| DEFAULT.to_owned());
        match std::fs::write(&file, bound(&text, section, control, &new)) {
            Ok(()) => {
                tracing::info!(what, key = %new, "keys file: a key changed in the in-game menu");
                // Read again at the next poll.
                now.looked = None;
                now.changed = None;
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "keys file: the key could not be kept");
                false
            }
        }
    }
    fn reset(&self) -> bool {
        let mut now = NOW.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(file) = now.file.clone() else { return false };
        let kept = file.with_file_name("keys-before-reset.txt");
        let _ = std::fs::copy(&file, &kept);
        match std::fs::write(&file, DEFAULT) {
            Ok(()) => {
                tracing::info!(kept = %kept.display(), "keys file: GTA V's keys again (the in-game menu)");
                now.looked = None;
                now.changed = None;
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "keys file: the keys could not be put back");
                false
            }
        }
    }
}

/// The layout that holds now, without a key seen (no switch is made).
pub(super) fn current() -> Layout {
    let mut now = NOW.lock().unwrap_or_else(PoisonError::into_inner);
    now.look(Instant::now());
    match &now.binds {
        Some(b) if now.driving => b.drive.clone(),
        Some(b) => b.walk.clone(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own() -> Binds {
        let (binds, wrong) = Binds::read(DEFAULT);
        assert!(wrong.is_empty(), "{wrong:?}");
        binds
    }

    fn keys_of(layout: &Layout, part: Part) -> Vec<u8> {
        layout.iter().filter(|b| b.1 == part).map(|b| b.0).collect()
    }

    /// Both layouts reach every control of the pad, and no key of one does
    /// two things at once.
    #[test]
    fn the_layouts_vilocity_comes_with_cover_the_pad() {
        let binds = own();
        for layout in [&binds.walk, &binds.drive] {
            for (name, part) in PARTS {
                assert!(!keys_of(layout, part).is_empty(), "{name}");
            }
            let mut keys: Vec<u8> = layout.iter().map(|b| b.0).collect();
            let all = keys.len();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), all, "a key does two things");
            assert!(!keys.contains(&binds.switch[0]));
        }
        // (Vilocity) Every key that has a name does something in both layouts, but the
        // switch (G), F1 (Vilocity's settings), F3 and F8 (the frame rate panels), and
        // Right Alt (AltGr: Windows presses Ctrl with it, which crouches).
        for layout in [&binds.walk, &binds.drive] {
            let named = KEYS.iter().map(|k| k.1).chain(b'A'..=b'Z').chain(b'0'..=b'9');
            let free: Vec<u8> = named.filter(|vk| !layout.iter().any(|b| b.0 == *vk)).collect();
            assert_eq!(free, [0x70, 0x72, 0x77, 0xA5, b'G'], "keys that do nothing");
        }
        // Aiming and firing on foot; W is the accelerator at the wheel.
        assert_eq!(keys_of(&binds.walk, Part::LeftTrigger)[0], 0x02);
        assert_eq!(keys_of(&binds.walk, Part::RightTrigger)[0], 0x01);
        // Running on foot is Shift (and jumping Space).
        assert_eq!(keys_of(&binds.walk, Part::Button(LEFT_STICK))[0], 0xA0);
        assert_eq!(keys_of(&binds.walk, Part::Button(A))[0], b' ');
        // (Vilocity) GTA V's keys on GTA VI's own controls page (Settings, Control Scheme
        // "Default", 2026-11-28): Ctrl crouches (B), R is melee (X), Q takes cover (RB).
        assert_eq!(keys_of(&binds.walk, Part::Button(B))[0], 0xA2);
        assert_eq!(keys_of(&binds.walk, Part::Button(X))[0], b'R');
        assert_eq!(keys_of(&binds.walk, Part::Button(RIGHT_SHOULDER))[0], b'Q');
        assert_eq!(keys_of(&binds.walk, Part::Button(Y))[0], b'F');
        // At the wheel: W and S, Space the handbrake (A), E the horn (L3), H the headlights
        // (B), F gets out (Y), a left click shoots (RB).
        assert_eq!(keys_of(&binds.drive, Part::RightTrigger)[0], b'W');
        assert_eq!(keys_of(&binds.drive, Part::LeftTrigger)[0], b'S');
        assert_eq!(keys_of(&binds.drive, Part::Button(A))[0], b' ');
        assert_eq!(keys_of(&binds.drive, Part::Button(LEFT_STICK))[0], b'E');
        assert_eq!(keys_of(&binds.drive, Part::Button(B))[0], b'H');
        assert_eq!(keys_of(&binds.drive, Part::Button(Y))[0], b'F');
        assert_eq!(keys_of(&binds.drive, Part::Button(RIGHT_SHOULDER))[0], 0x01);
        assert!(!keys_of(&binds.drive, Part::Button(Y)).contains(&b'E'), "E is the horn, not the way out");
        assert_eq!(binds.enter_exit, [b'F']);
    }

    #[test]
    fn a_file_changes_what_it_names_and_says_what_it_does_not_know() {
        let (binds, wrong) =
            Binds::read("[general]\nswitch = f5 # comment\n[walk]\nA = Space, nokey\nnothing\nQQ = 1\n[fly]\nA = B\n");
        assert_eq!(binds.switch, [0x74]);
        assert_eq!(binds.walk, [(0x20, Part::Button(A))]);
        // Not named: as it comes.
        assert_eq!(binds.drive, own().drive);
        let lines: Vec<usize> = wrong.iter().map(|w| w.0).collect();
        assert_eq!(lines, [4, 5, 6, 7, 8]);
        // An empty file is the one Vilocity comes with.
        assert_eq!(Binds::read("").0, own());
        // Several keys for one control, and one named empty: no key.
        let (binds, _) = Binds::read("[general]\nSWITCH =\n[drive]\nRB = Space, Ctrl\n");
        assert!(binds.switch.is_empty());
        assert_eq!(keys_of(&binds.drive, Part::Button(RIGHT_SHOULDER)), [0x20, 0xA2]);
    }

    #[test]
    fn the_switch_key_changes_the_layout_once_a_press() {
        let mut now = Now {
            file: None,
            looked: None,
            changed: None,
            binds: None,
            driving: false,
            switch_was_down: false,
            enter_was_down: false,
            pending: None,
        };
        let at = Instant::now();
        let is_drive = |layout: Layout| keys_of(&layout, Part::RightTrigger)[0] == b'W';
        assert!(!is_drive(now.layout(at, |_| false)));
        assert!(is_drive(now.layout(at, |vk| vk == b'G')));
        // Held: no second change.
        assert!(is_drive(now.layout(at, |vk| vk == b'G')));
        assert!(is_drive(now.layout(at, |_| false)));
        assert!(!is_drive(now.layout(at, |vk| vk == b'G')));
    }

    #[test]
    fn getting_in_with_f_gives_the_driving_keys_after_the_get_in_and_out_at_once() {
        let mut now = Now {
            file: None,
            looked: None,
            changed: None,
            binds: None,
            driving: false,
            switch_was_down: false,
            enter_was_down: false,
            pending: None,
        };
        let at = Instant::now();
        let is_drive = |layout: Layout| keys_of(&layout, Part::RightTrigger)[0] == b'W';
        // F: still walking during the get-in, driving after it.
        assert!(!is_drive(now.layout(at, |vk| vk == b'F')));
        assert!(!is_drive(now.layout(at + Duration::from_millis(1000), |_| false)));
        assert!(is_drive(now.layout(at + GET_IN, |_| false)));
        // F again: walking at once.
        assert!(!is_drive(now.layout(at + GET_IN, |vk| vk == b'F')));
        // A get-in that did not happen is put right by G, which also drops a waiting one.
        let later = at + GET_IN * 4;
        now.layout(later, |_| false);
        now.layout(later, |vk| vk == b'F');
        assert!(is_drive(now.layout(later, |vk| vk == b'G')));
        assert!(!is_drive(now.layout(later, |vk| vk == b'G' || vk == b'F')), "F gets out again");
        assert!(!is_drive(now.layout(later + GET_IN * 2, |_| false)));
        // E (interact, the horn at the wheel) never switches.
        assert!(!is_drive(now.layout(later + GET_IN * 3, |vk| vk == b'E')));
        assert!(!is_drive(now.layout(later + GET_IN * 5, |_| false)));
    }

    #[test]
    fn the_file_vilocity_made_before_is_known_and_the_new_one_differs() {
        let (before, wrong) = Binds::read(BEFORE);
        assert!(wrong.is_empty(), "{wrong:?}");
        assert_ne!(before, own());
        assert!(DEFAULT.contains("(vilocity keys 2)") && !BEFORE.contains("(vilocity keys 2)"));
    }

    #[test]
    fn the_menu_changes_a_key_in_the_file_and_takes_it_from_the_others() {
        // Run on Ctrl: Ctrl first for L3 on foot, gone from crouch there, the driving keys as they were.
        let text = bound(DEFAULT, "walk", "L3", "Ctrl");
        let (binds, wrong) = Binds::read(&text);
        assert!(wrong.is_empty(), "{wrong:?}");
        assert_eq!(keys_of(&binds.walk, Part::Button(LEFT_STICK))[0], 0xA2);
        assert!(!keys_of(&binds.walk, Part::Button(B)).contains(&0xA2));
        assert_eq!(binds.drive, own().drive);
        assert_eq!(keys_of_control(&text, "walk", "L3")[0], "Ctrl");
        // Pause on both layouts; the get-in key in [general].
        let text = bound(&text, "both", "MENU", "Backspace");
        let (binds, _) = Binds::read(&text);
        assert_eq!(keys_of(&binds.walk, Part::Button(MENU))[0], 0x08);
        assert_eq!(keys_of(&binds.drive, Part::Button(MENU))[0], 0x08);
        let (binds, _) = Binds::read(&bound(&text, "general", "ENTER_EXIT", "Y"));
        assert_eq!(binds.enter_exit, [b'Y']);
        // A control a file lacks is added to its section.
        let (binds, _) = Binds::read(&bound("[general]\nSWITCH = G\n[walk]\nA = Space\n[drive]\nRT = W\n", "walk", "B", "C"));
        assert_eq!(keys_of(&binds.walk, Part::Button(B)), [b'C']);
        // Every row names a control of the file Vilocity makes, with a key.
        for (what, section, control) in ROWS {
            let section = if section == "both" { "walk" } else { section };
            assert!(!keys_of_control(DEFAULT, section, control).is_empty(), "{what}");
        }
        assert_eq!(name(0xA2).as_deref(), Some("Ctrl"));
        assert_eq!(name(b'Q').as_deref(), Some("Q"));
        assert_eq!(name(0xA4), None);
    }
}
