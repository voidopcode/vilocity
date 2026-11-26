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
const KEYS: [(&str, u8); 43] = [
    ("LeftClick", 0x01),
    ("RightClick", 0x02),
    ("MiddleClick", 0x04),
    ("Mouse4", 0x05),
    ("Mouse5", 0x06),
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0D),
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
    ("F11", 0x7A),
    ("Shift", 0xA0),
    ("RightShift", 0xA1),
    ("Ctrl", 0xA2),
    ("RightCtrl", 0xA3),
    ("RightAlt", 0xA5),
    ("Semicolon", 0xBA),
    ("Comma", 0xBC),
    ("Minus", 0xBD),
    ("Period", 0xBE),
    ("Slash", 0xBF),
    ("Backquote", 0xC0),
];

/// The file as it is made when there is none, and what holds without one.
pub(super) const DEFAULT: &str = "\
# Vilocity keys. Change the keys on the right of '='; save; the running title
# takes them within a second. Several keys: separate them with commas.
# Keys: letters, digits, F1 to F11, Shift, RightShift, Ctrl, RightCtrl,
# RightAlt, Space, Enter, Esc, Tab, CapsLock, Backspace, Insert, Delete, Home,
# End, PageUp, PageDown, ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Comma,
# Period, Slash, Semicolon, Minus, Backquote, LeftClick, RightClick,
# MiddleClick, Mouse4, Mouse5. The mouse's motion is always the right stick.
# Left Alt gives the mouse back and cannot be used. Delete this file to get
# the one Vilocity comes with again.

[general]
# Switches between the two layouts below (the window's log says which).
SWITCH = G

[walk]
MOVE_UP = W
MOVE_DOWN = S
MOVE_LEFT = A
MOVE_RIGHT = D
LT = RightClick, U
RT = LeftClick, O
A = Shift, Enter
B = R, Backspace
X = Space, X
Y = F, Y
LB = Tab, Mouse4
RB = Q, Mouse5
L3 = Ctrl, Z
R3 = C, MiddleClick, H
UP = ArrowUp, 1
DOWN = ArrowDown, 2
LEFT = ArrowLeft, 3
RIGHT = ArrowRight, 4, E
MENU = Esc, P
VIEW = V
LOOK_UP = I
LOOK_DOWN = K
LOOK_LEFT = J
LOOK_RIGHT = L

[drive]
# W and S are the triggers (accelerate, brake), A and D steer.
RT = W
LT = S
MOVE_LEFT = A
MOVE_RIGHT = D
MOVE_UP = ArrowUp
MOVE_DOWN = ArrowDown
LB = LeftClick, RightClick
RB = Space
A = Shift, Enter
B = R, Backspace
X = X
Y = F, Y
L3 = Ctrl, H
R3 = C, MiddleClick
UP = 1
DOWN = 2
LEFT = 3, ArrowLeft
RIGHT = 4, E, ArrowRight
MENU = Esc, P
VIEW = V
LOOK_UP = I
LOOK_DOWN = K
LOOK_LEFT = J
LOOK_RIGHT = L
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

/// The two layouts and the key that switches them.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Binds {
    pub walk: Layout,
    pub drive: Layout,
    pub switch: Vec<u8>,
}

impl Binds {
    /// Reads a file's text: what it says, and the lines it does not
    /// understand (number from 1, why). A section the file leaves out, or
    /// leaves empty, is the one Vilocity comes with.
    pub fn read(text: &str) -> (Binds, Vec<(usize, String)>) {
        let mut binds = Binds { walk: Vec::new(), drive: Vec::new(), switch: Vec::new() };
        let mut wrong = Vec::new();
        let mut section = String::new();
        let mut named_switch = false;
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
        if binds.walk.is_empty() || binds.drive.is_empty() || !named_switch {
            // (`DEFAULT` names all three: no endless reading.)
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
}

static NOW: Mutex<Now> = Mutex::new(Now {
    file: None,
    looked: None,
    changed: None,
    binds: None,
    driving: false,
    switch_was_down: false,
});

/// How often the file is looked at for changes.
const LOOK: Duration = Duration::from_secs(1);

/// The player's keys are in `file`, which is made if it is not there.
pub fn use_file(file: PathBuf) {
    if !file.exists()
        && let Err(e) = std::fs::write(&file, DEFAULT)
    {
        tracing::warn!(error = %e, file = %file.display(), "the keys file could not be made: the keys Vilocity comes with hold");
    }
    tracing::info!(file = %file.display(), "keys of the stand-in pad: change them in this file");
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
        let pressed = binds.switch.iter().any(|&vk| down(vk));
        if pressed && !self.switch_was_down {
            self.driving = !self.driving;
            tracing::info!(layout = if self.driving { "drive" } else { "walk" }, "keys");
        }
        self.switch_was_down = pressed;
        if self.driving { binds.drive.clone() } else { binds.walk.clone() }
    }
}

/// The layout that holds now; a press of the switch key, seen here, changes
/// it.
pub(super) fn layout(down: impl Fn(u8) -> bool) -> Layout {
    NOW.lock().unwrap_or_else(PoisonError::into_inner).layout(Instant::now(), down)
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
        // Aiming and firing on foot; W is the accelerator at the wheel.
        assert_eq!(keys_of(&binds.walk, Part::LeftTrigger)[0], 0x02);
        assert_eq!(keys_of(&binds.walk, Part::RightTrigger)[0], 0x01);
        assert_eq!(keys_of(&binds.walk, Part::Button(LEFT_STICK))[0], 0xA2);
        assert_eq!(keys_of(&binds.drive, Part::RightTrigger), [b'W']);
        assert_eq!(keys_of(&binds.drive, Part::LeftTrigger), [b'S']);
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
        let mut now =
            Now { file: None, looked: None, changed: None, binds: None, driving: false, switch_was_down: false };
        let at = Instant::now();
        let is_drive = |layout: Layout| keys_of(&layout, Part::RightTrigger) == [b'W'];
        assert!(!is_drive(now.layout(at, |_| false)));
        assert!(is_drive(now.layout(at, |vk| vk == b'G')));
        // Held: no second change.
        assert!(is_drive(now.layout(at, |vk| vk == b'G')));
        assert!(is_drive(now.layout(at, |_| false)));
        assert!(!is_drive(now.layout(at, |vk| vk == b'G')));
    }
}
