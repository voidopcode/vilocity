//! D3D12.X on host D3D12.
//!
//! D3D12.X records commands from code inlined into the title, which writes
//! packets into the command list while its put pointer is below a limit and
//! calls the list's method otherwise. Our lists keep the limits at zero, so
//! every command arrives here as a call, to be run on host D3D12. The
//! title's GPU addresses are CPU addresses (console memory is unified).
//!
//! [`device`] is the device object titles create, [`dxgi`] its adapter side,
//! [`resource`] heaps and resources, [`fence`] fences, [`queue`] command
//! queues, [`list`] command allocators and lists, [`descriptor`]
//! descriptor heaps, [`root`] root signatures, [`query`] query heaps,
//! [`signature`] command signatures, [`pipeline`] pipeline states,
//! [`frame`] frame pacing, [`object`] what all console graphics objects
//! share, [`console`] which console model the title sees, `external` the
//! host objects for PC libraries the title's calls are redirected to.
//! Planned: `present` (swap chain).

#[cfg(windows)]
mod ambient;
#[cfg(all(windows, feature = "diagnostics"))]
mod capture;
#[cfg(windows)]
mod compose;
pub mod console;
#[cfg(all(windows, feature = "diagnostics"))]
mod copystat;
#[cfg(all(windows, feature = "diagnostics"))]
mod faultbook;
#[cfg(windows)]
mod cores;
pub mod descriptor;
pub mod device;
#[cfg(windows)]
mod draw;
pub mod dxgi;
#[cfg(windows)]
pub mod external;
pub mod fence;
pub mod frame;
#[cfg(all(windows, feature = "diagnostics"))]
mod book;
#[cfg(windows)]
mod host;
#[cfg(windows)]
mod indirect;
#[cfg(windows)]
mod inplace;
#[cfg(windows)]
mod land;
mod live;
pub mod list;
pub mod object;
pub mod pipeline;
#[cfg(windows)]
mod picked;
#[cfg(windows)]
mod pool;
#[cfg(all(windows, feature = "diagnostics"))]
mod popwatch;
#[cfg(windows)]
mod primitive;
pub mod query;
pub mod queue;
#[cfg(all(windows, feature = "diagnostics"))]
pub mod replay;
#[cfg(windows)]
mod residency;
pub mod resource;
pub mod root;
#[cfg(windows)]
mod safecopy;
#[cfg(windows)]
mod sampler;
#[cfg(windows)]
pub mod shader_cache;
#[cfg(windows)]
mod memory;
#[cfg(windows)]
mod shadow;
#[cfg(windows)]
mod tiles;
pub mod signature;
// (Vilocity: DLSS through Streamline is louketa/foxhound's, lucas-3080 56dba41, GPL-3.0-or-later, as this.)
#[cfg(windows)]
pub mod streamline;
// (Vilocity) AMD FSR frame generation (`VILOCITY_FSR_FRAMEGEN=1`).
#[cfg(windows)]
pub mod fsr_fg;
#[cfg(windows)]
mod volume;
mod switch;
#[cfg(all(windows, feature = "diagnostics"))]
mod ab;
#[cfg(windows)]
mod tdr;
#[cfg(windows)]
mod texture;
pub mod timeline;
#[cfg(windows)]
pub mod titlewrite;
#[cfg(windows)]
mod upload;
#[cfg(windows)]
mod watch;
#[cfg(windows)]
mod texback;

#[cfg(windows)]
pub use host::on_failure;
#[cfg(windows)]
pub use cores::{keep_beside, title_on_its_die};

/// Whether `switch` is on now, if it is one of those `VILOCITY_AB` turns on and
/// off while the title runs (`ab`).
#[cfg(all(windows, feature = "diagnostics"))]
pub fn switched(switch: &str) -> Option<bool> {
    ab::pick(switch)
}

/// What the frame is at (`copystat::phase`), for the sampling profiler.
#[cfg(all(windows, feature = "diagnostics"))]
pub fn frame_phase() -> u32 {
    copystat::phase()
}

/// Whether the in-game menu (F1) is shown, or was just closed with a key
/// still held: the title is to get no keys and no mouse.
#[cfg(windows)]
pub fn menu_holds_input() -> bool {
    host::menu::holds_input()
}

/// (Vilocity) The mouse as the in-game menu has it: sensitivity, up and down inverted,
/// smoothing (frames); the input reads it at each poll.
pub fn mouse_settings() -> (f32, bool, f32) {
    live::mouse()
}

/// (Vilocity) What the in-game menu's keys page shows and changes; the input's (its keys
/// file) gives it at the start ([`set_keys_page`]).
pub trait KeysPage: Send + Sync {
    /// The rows: what a key does ("On foot: run"), and its key now.
    fn rows(&self) -> Vec<(String, String)>;
    /// Makes virtual key `vk` the key of `row` (in its file); false if `vk` has no name there.
    fn bind(&self, row: usize, vk: u8) -> bool;
    /// Every key as Vilocity comes (the file kept beside); false if it could not be.
    fn reset(&self) -> bool {
        false
    }
}

static KEYS_PAGE: std::sync::OnceLock<Box<dyn KeysPage>> = std::sync::OnceLock::new();

/// (Vilocity) The keys page, once.
pub fn set_keys_page(page: Box<dyn KeysPage>) {
    let _ = KEYS_PAGE.set(page);
}

pub(crate) fn keys_page() -> Option<&'static dyn KeysPage> {
    KEYS_PAGE.get().map(|p| p.as_ref())
}

/// Frames the title has begun (its frame waits that ended), for logs.
pub fn frames_begun() -> u64 {
    frame::current()
}
#[cfg(windows)]
pub use inplace::{title_memory_freeing, title_memory_section, title_memory_section_gone, title_memory_view};
#[cfg(windows)]
pub use watch::{
    kernel_writing, title_memory_allocated, title_memory_freed, title_memory_reading, title_memory_remapped,
    title_memory_remapping, title_memory_splitting, title_memory_unmapping, title_memory_written,
    writing as kernel_will_write,
};
