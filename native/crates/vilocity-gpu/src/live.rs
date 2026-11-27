//! Settings the in-game menu (F9, `host::menu`) changes while the game runs.
//! Each is read where it is used, at each use: a change shows in the next
//! frame. Until the menu sets one, its variable from the start counts
//! (`VILOCITY_...`), as before the menu.
//!
//! What the menu set is kept in `menu.txt` in the data folder (the sandbox
//! may write there), one `name=value` a line, and set again at the next start;
//! the menu's "reset" forgets it, and the variables count again.

use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Once;

/// The settings, by the place of their value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Knob {
    /// The light stand-in's share in the shade (`VILOCITY_AMBIENT`; 0: the title's own).
    Light,
    /// The least light it gives at night (`VILOCITY_AMBIENT_FLOOR`; 0: none).
    Night,
    /// What the air may scatter, a multiple of the light around (`VILOCITY_AMBIENT_AIR`; 0: the title's).
    Fog,
    /// The stand-in for reflections (`VILOCITY_AMBIENT_SPECULAR`; 0: off, 1: on).
    Reflections,
    /// At most one submission ahead of what is sent (`VILOCITY_SUBMIT_DEPTH=1`; 0: off, 1: on).
    Steady,
    /// Frames a second at most (0: no cap).
    Cap,
    /// The upscaler at the next start (`scripts\play-gta6.ps1` reads it): 0 the
    /// game's own (AMD FSR), 1 DLSS quality, 2 DLSS balanced, 3 DLSS performance.
    Upscaler,
    /// DLSS frame generation: 0 off, 2 to 4 frames shown for each one rendered;
    /// live when it was on at the start, else from the next start.
    FrameGen,
    /// The picture at the next start: 0 the Series S one (1080p), 1 the
    /// Series X one at 2560x1440, 2 the Series X one at 3840x2160.
    Picture,
    /// The DLSS model at the next start: 0 NVIDIA's default, 1 J, 2 K, 3 L, 4 M.
    DlssModel,
    /// The title's heat haze at the next start: 0 as the game has it, 1 left out.
    Haze,
    /// The torn-frame fix at the next start (`VILOCITY_CARD_STEPS=0`): 0 off, 1 on.
    TornFix,
    /// How strongly the light stand-in shades creases (`VILOCITY_AMBIENT_SHADE`'s
    /// first value; 1 as it is, 0 none).
    Shade,
}

pub(crate) const KNOBS: [Knob; 13] = [
    Knob::Light,
    Knob::Night,
    Knob::Fog,
    Knob::Reflections,
    Knob::Steady,
    Knob::Cap,
    Knob::Upscaler,
    Knob::FrameGen,
    Knob::Picture,
    Knob::DlssModel,
    Knob::Haze,
    Knob::TornFix,
    Knob::Shade,
];

impl Knob {
    /// Its name in `menu.txt`.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Knob::Light => "light",
            Knob::Night => "night",
            Knob::Fog => "fog",
            Knob::Reflections => "reflections",
            Knob::Steady => "steady",
            Knob::Cap => "cap",
            Knob::Upscaler => "upscaler",
            Knob::FrameGen => "framegen",
            Knob::Picture => "picture",
            Knob::DlssModel => "dlssmodel",
            Knob::Haze => "haze",
            Knob::TornFix => "tornfix",
            Knob::Shade => "shade",
        }
    }
}

/// No value set: the variable counts.
const UNSET: u32 = u32::MAX;

static VALUES: [AtomicU32; KNOBS.len()] = [const { AtomicU32::new(UNSET) }; KNOBS.len()];

/// The menu's value of `knob`, if it set one (this run or a run before).
pub(crate) fn get(knob: Knob) -> Option<f32> {
    load();
    let bits = VALUES[knob as usize].load(Relaxed);
    (bits != UNSET).then(|| f32::from_bits(bits))
}

/// Sets `knob` from here on (`None`: the variable counts again), and keeps
/// all of them for the next start.
pub(crate) fn set(knob: Knob, value: Option<f32>) {
    load();
    VALUES[knob as usize].store(value.filter(|v| v.is_finite()).map_or(UNSET, f32::to_bits), Relaxed);
    save();
}

/// `menu.txt` in the data folder.
fn file() -> Option<std::path::PathBuf> {
    std::env::var_os("VILOCITY_DATA_DIR").map(|d| std::path::PathBuf::from(d).join("menu.txt"))
}

/// The values kept, once.
fn load() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else {
            return;
        };
        for (knob, value) in parse(&text) {
            VALUES[knob as usize].store(value.to_bits(), Relaxed);
        }
        tracing::info!("the in-game menu's settings of the last start: {}", text.trim().replace('\n', ", "));
    });
}

fn parse(text: &str) -> Vec<(Knob, f32)> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let knob = *KNOBS.iter().find(|k| k.key() == key.trim())?;
            let value = value.trim().parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0)?;
            Some((knob, value))
        })
        .collect()
}

fn save() {
    let Some(path) = file() else { return };
    let text: String = KNOBS
        .iter()
        .filter_map(|&k| {
            let bits = VALUES[k as usize].load(Relaxed);
            (bits != UNSET).then(|| format!("{}={}\n", k.key(), f32::from_bits(bits)))
        })
        .collect();
    let written = if text.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        std::fs::write(&path, text)
    };
    if let Err(e) = written {
        tracing::warn!(error = %e, "the in-game menu's settings could not be kept for the next start");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_settings_are_read_back_and_bad_lines_left_out() {
        let read = parse("light=0.6\nnight=0.05\nfog=x\nunknown=1\ncap = 60\nsteady=-1\n\nreflections=1");
        assert_eq!(read, vec![(Knob::Light, 0.6), (Knob::Night, 0.05), (Knob::Cap, 60.0), (Knob::Reflections, 1.0)]);
    }

    #[test]
    fn every_knob_has_its_own_name() {
        for (i, a) in KNOBS.iter().enumerate() {
            assert_eq!(*a as usize, i, "the order of KNOBS is the order of the values");
            for b in &KNOBS[i + 1..] {
                assert_ne!(a.key(), b.key());
            }
        }
    }
}
