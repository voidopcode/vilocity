//! (Vilocity: DLSS through Streamline is louketa/foxhound's, lucas-3080 56dba41, GPL-3.0-or-later, as this.)
//!
//! NVIDIA DLSS Super Resolution and DLSS Frame Generation through NVIDIA
//! Streamline, loaded at run time from a folder of the user's; both off by
//! default.
//!
//! - `VILOCITY_DLSS=1`: the title's FidelityFX upscaling (its `ffxDispatch`,
//!   served by `vilocity_hle::modules::fidelityfx::dlss`) is done by DLSS
//!   instead, from the same colour, depth, motion vectors, exposure and
//!   reactive mask into the same output, at the same place of the title's
//!   command list.
//!   `VILOCITY_DLSS_MODE` (dlaa, quality, balanced, performance,
//!   ultraperformance) is the render scale the app patches the title's
//!   renderer to; the DLSS mode follows the sizes the title then passes.
//!   `VILOCITY_DLSS_PRESET` (default, j, k, l, m) picks the model.
//! - `VILOCITY_FRAMEGEN=1`, with `VILOCITY_DLSS=1`: DLSS Frame Generation
//!   (2x) on our present. The
//!   device's direct queue and the swap chain are made through Streamline's
//!   proxies; every present tags the depth and motion vectors of the last
//!   upscaling submitted before it, with constants made from it, and sends
//!   the latency markers Frame Generation needs (Reflex on). Needs the line
//!   of submissions (`VILOCITY_THIN=1` or `VILOCITY_ASYNC_SUBMIT=1`).
//! - `VILOCITY_STREAMLINE_DIR`: the folder of `sl.interposer.dll`, its plugins
//!   and NGX's DLLs. It must be named: there is no folder looked in without
//!   it.
//!
//! Anything missing (the folder, a signature, a feature on this GPU, the
//! device hand-over) leaves the title's own upscaling and our plain present
//! as they were, with a warning in the log.
//!
//! Streamline is used with manual hooking: nothing of D3D12 or DXGI is
//! hooked globally; only the objects handed to `slUpgradeInterface` are
//! proxies (the device that makes the direct queue, the factory that makes
//! the swap chain, and what they make).
//!
//! What differs from foxhound's here (Vilocity): nothing is read or loaded
//! without `VILOCITY_DLSS=1` ([`Asked::from_env`]; foxhound's frame generation
//! alone and its default folder are left out), and the upscaling is recorded
//! onto the title's list as this tree records a PC library's work
//! (`crate::external::record`, [`upscale`]), with its textures made ready as
//! that module makes them ready.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub(crate) mod abi;
pub mod plan;

pub use plan::Mode;

use std::ffi::{CStr, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::*};
use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::Interface;

use crate::object::Object;

const INTERPOSER: &str = "sl.interposer.dll";
/// The project id NGX is given (engine "custom", no NVIDIA application id).
const PROJECT_ID: &CStr = c"6f0d7b2e-3a41-4c1e-9b57-2d8e4f1a9c30";
const ENGINE_VERSION: &CStr = c"Vilocity 0.1";
/// The viewport of the upscaling; frame generation wants 0.
const VIEWPORT_DLSS: u32 = 1;
const VIEWPORT_FG: u32 = 0;

/// What the user asked for, read from the environment once.
#[derive(Clone, Debug)]
pub struct Asked {
    pub dlss: bool,
    pub framegen: bool,
    pub mode: Mode,
    pub preset: u32,
    /// `VILOCITY_STREAMLINE_DIR`; `None`: not named, and nothing is loaded.
    pub dir: Option<PathBuf>,
    /// `VILOCITY_DLSS_JITTER` and `VILOCITY_DLSS_MV`: sign or scale fixes.
    pub jitter: [f32; 2],
    pub mvec: [f32; 2],
    /// `VILOCITY_DLSS_REACTIVE=0`: the reactive mask is not given to DLSS.
    pub reactive: bool,
    /// `VILOCITY_STREAMLINE_LOG=<folder>`: Streamline's own log there, verbose.
    pub log_dir: Option<PathBuf>,
    /// `VILOCITY_FRAMEGEN_HUDLESS=0`: with frame generation, no colour
    /// without the interface is copied or tagged where the title draws its
    /// interface into the frame (Series X).
    pub hudless: bool,
    /// `VILOCITY_STREAMLINE_PRESENT=1`: with DLSS and no frame generation,
    /// our swap chain is Streamline's proxy of it, so that Streamline sees
    /// the presents (its bookkeeping per frame).
    pub present_hook: bool,
}

impl Asked {
    /// The switches; with `VILOCITY_DLSS=1` an unknown mode or preset name
    /// is an error. `forced`: as with `VILOCITY_DLSS=1` ([`check`]).
    pub fn from_env(forced: bool) -> Result<Self, String> {
        Self::read(|name| std::env::var(name).ok(), forced)
    }

    /// [`Asked::from_env`] of the variables `var` gives. (Vilocity) Without
    /// `VILOCITY_DLSS=1` everything is off, frame generation too (foxhound
    /// runs it alone), and a mode or preset name is not looked at: a run
    /// without the switch cannot fail or change for the other variables.
    fn read(var: impl Fn(&str) -> Option<String>, forced: bool) -> Result<Self, String> {
        let on = |name: &str| var(name).is_some_and(|v| v.trim() == "1");
        let dlss = forced || on("VILOCITY_DLSS");
        let framegen = dlss && on("VILOCITY_FRAMEGEN");
        let mode = match var("VILOCITY_DLSS_MODE").filter(|_| dlss) {
            Some(name) => Mode::from_name(&name).ok_or_else(|| {
                format!("VILOCITY_DLSS_MODE={name}: not one of dlaa, quality, balanced, performance, ultraperformance")
            })?,
            None => Mode::Quality,
        };
        let preset = match var("VILOCITY_DLSS_PRESET").filter(|_| dlss) {
            Some(name) => plan::preset(&name)
                .ok_or_else(|| format!("VILOCITY_DLSS_PRESET={name}: not one of default, e, f, j, k, l, m"))?,
            None => 0,
        };
        Ok(Asked {
            dlss,
            framegen,
            mode,
            preset,
            dir: var("VILOCITY_STREAMLINE_DIR").filter(|v| !v.trim().is_empty()).map(PathBuf::from),
            jitter: plan::pair(var("VILOCITY_DLSS_JITTER").as_deref()),
            mvec: plan::pair(var("VILOCITY_DLSS_MV").as_deref()),
            reactive: var("VILOCITY_DLSS_REACTIVE").is_none_or(|v| v.trim() != "0"),
            log_dir: var("VILOCITY_STREAMLINE_LOG").filter(|v| !v.trim().is_empty() && v.trim() != "0").map(PathBuf::from),
            hudless: var("VILOCITY_FRAMEGEN_HUDLESS").is_none_or(|v| v.trim() != "0"),
            present_hook: on("VILOCITY_STREAMLINE_PRESENT"),
        })
    }

    pub fn any(&self) -> bool {
        self.dlss || self.framegen
    }
}

/// What [`init`] made available: the upscaling (the title's renderer may be
/// patched to the FidelityFX path for it) and frame generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Available {
    pub dlss: bool,
    pub framegen: bool,
}

/// One upscaling the title asked for: a FidelityFX `ffxDispatch` of an
/// upscale, in its own terms (`vilocity_hle::modules::fidelityfx::dlss` reads it).
#[derive(Clone, Copy, Debug)]
pub struct Upscale {
    /// The FidelityFX context.
    pub context: usize,
    /// The title's command list.
    pub list: *const Object,
    /// Colour, depth, motion vectors, exposure, reactive mask, transparency
    /// and composition mask, output: the title's resource (null if none) and
    /// the FidelityFX state it is in.
    pub resources: [(*const Object, u32); 7],
    pub camera: plan::Camera,
    pub pre_exposure: f32,
    pub frame_ms: f32,
}

pub const COLOUR: usize = 0;
pub const DEPTH: usize = 1;
pub const MOTION: usize = 2;
pub const EXPOSURE: usize = 3;
pub const REACTIVE: usize = 4;
pub const OUTPUT: usize = 6;

/// What became of an upscaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    /// DLSS does it: the title's own must not run.
    Dlss,
    /// Only noted for frame generation: the title's own runs.
    Noted,
    /// Neither: the title's own runs. Why, for the log.
    No(&'static str),
}

macro_rules! once {
    ($level:ident, $($arg:tt)*) => {{
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| tracing::$level!($($arg)*));
    }};
}

struct Library(windows_sys::Win32::Foundation::HMODULE);

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: this owns the reference LoadLibraryExW took.
        unsafe { windows_sys::Win32::Foundation::FreeLibrary(self.0) };
    }
}

// SAFETY: a module handle.
unsafe impl Send for Library {}
// SAFETY: as above.
unsafe impl Sync for Library {}

/// The library's exported functions.
struct Sl {
    _library: Library,
    init: abi::SlInit,
    is_feature_supported: abi::SlIsFeatureSupported,
    evaluate_feature: abi::SlEvaluateFeature,
    free_resources: abi::SlFreeResources,
    set_tag_for_frame: abi::SlSetTagForFrame,
    get_feature_requirements: abi::SlGetFeatureRequirements,
    get_feature_version: abi::SlGetFeatureVersion,
    upgrade_interface: abi::SlUpgradeInterface,
    set_constants: abi::SlSetConstants,
    get_native_interface: abi::SlGetNativeInterface,
    get_feature_function: abi::SlGetFeatureFunction,
    get_new_frame_token: abi::SlGetNewFrameToken,
    set_d3d_device: abi::SlSetD3DDevice,
    shutdown: abi::SlShutdown,
}

static SL: OnceLock<Sl> = OnceLock::new();

/// What is on once the library is in: the switches and the features found.
struct Plan {
    asked: Asked,
    dlss: bool,
    framegen: bool,
}

static PLAN: OnceLock<Plan> = OnceLock::new();

/// The per-feature functions, once the device is set.
#[derive(Default)]
struct Functions {
    dlss_optimal: Option<abi::SlDlssGetOptimalSettings>,
    dlss_set_options: Option<abi::SlDlssSetOptions>,
    dlssg_set_options: Option<abi::SlDlssgSetOptions>,
    dlssg_get_state: Option<abi::SlDlssgGetState>,
    reflex_set_options: Option<abi::SlReflexSetOptions>,
    pcl_set_marker: Option<abi::SlPclSetMarker>,
}

static FUNCTIONS: OnceLock<Functions> = OnceLock::new();
static DLSS_READY: AtomicBool = AtomicBool::new(false);
static FG_READY: AtomicBool = AtomicBool::new(false);

/// Streamline's proxies kept for frame generation: the device that made the
/// direct queue and the queue the swap chain is made on.
struct Proxies {
    _device: ID3D12Device,
    queue: ID3D12CommandQueue,
}

// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Proxies {}
// SAFETY: as above.
unsafe impl Sync for Proxies {}

static PROXIES: OnceLock<Proxies> = OnceLock::new();
/// Our present goes through Streamline's swap chain.
static SWAP_CHAIN_PROXIED: AtomicBool = AtomicBool::new(false);
/// Our swap chain is Streamline's proxy of it ([`present_hooked`]).
static PRESENT_HOOKED: AtomicBool = AtomicBool::new(false);

/// Library calls that are not thread safe, and what the upscaling keeps
/// between frames, one caller at a time.
#[derive(Default)]
struct Calls {
    /// Optimal render sizes by output size and mode.
    optimal: Vec<([u32; 2], u32, Option<[u32; 2]>)>,
    /// The mode, sizes and preset the last options named.
    last: Option<(u32, [u32; 2], [u32; 2])>,
    /// Frame generation: on, the frames shown for each rendered one last
    /// asked of it (0: off), and the frame whose simulation began.
    framegen_on: bool,
    framegen_times: u32,
    simulating: Option<u32>,
}

static CALLS: Mutex<Calls> = Mutex::new(Calls {
    optimal: Vec::new(),
    last: None,
    framegen_on: false,
    framegen_times: 0,
    simulating: None,
});

fn calls() -> std::sync::MutexGuard<'static, Calls> {
    CALLS.lock().unwrap_or_else(|e| e.into_inner())
}

// Counters since the last frame-rate line: upscalings done by DLSS, failed,
// left to the title; presents with frame generation on and off, frames it
// reported shown; and in all.
static EVALUATED: AtomicU64 = AtomicU64::new(0);
static FAILED: AtomicU64 = AtomicU64::new(0);
static FORWARDED: AtomicU64 = AtomicU64::new(0);
static FG_ON: AtomicU64 = AtomicU64::new(0);
static FG_OFF: AtomicU64 = AtomicU64::new(0);
static FG_SHOWN: AtomicU64 = AtomicU64::new(0);
static EVALUATED_ALL: AtomicU64 = AtomicU64::new(0);
static FG_STATUS: AtomicU32 = AtomicU32::new(0);
/// Presents with frame generation on, and frames it showed, in all.
static FG_ON_ALL: AtomicU64 = AtomicU64::new(0);
static FG_SHOWN_ALL: AtomicU64 = AtomicU64::new(0);
/// Upscalings noted so far (their ids).
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
/// [`check`] presents from its one thread, with nothing else submitting.
static CHECKING: AtomicBool = AtomicBool::new(false);

/// Nothing at all unless `asked` has DLSS on (`VILOCITY_DLSS=1`); then
/// loads Streamline from `asked.dir` and asks for what `asked` names; call
/// once, before the host device is made (`crate::host::host`) and before the
/// title's imports are bound. Fails soft: what is not there is logged and
/// left off.
pub fn init(asked: &Asked) -> Available {
    if !asked.any() {
        return Available::default();
    }
    match load_and_init(asked, true) {
        Ok(available) => available,
        Err(why) => {
            tracing::warn!(
                folder = %folder(asked).display(),
                "Streamline: {why}; DLSS and frame generation stay off, the title's own upscaling and our present run"
            );
            Available::default()
        }
    }
}

/// The folder asked for, for a message (empty: none named).
fn folder(asked: &Asked) -> &Path {
    asked.dir.as_deref().unwrap_or(Path::new(""))
}

fn load_and_init(asked: &Asked, require_line: bool) -> Result<Available, String> {
    let Some(dir) = asked.dir.as_deref() else {
        return Err("VILOCITY_STREAMLINE_DIR names no folder".into());
    };
    // (The whole path: the library and its plugins are looked for there alone.)
    let dir = std::path::absolute(dir).map_err(|e| e.to_string())?;
    let path = dir.join(INTERPOSER);
    if !path.is_file() {
        return Err(format!("no {INTERPOSER} in that folder (VILOCITY_STREAMLINE_DIR)"));
    }
    verify_signature(&path)?;
    let sl = load(&path)?;
    let sl = SL.get_or_init(|| sl);
    let dir_wide: Vec<u16> = wide(&dir);
    let paths = [dir_wide.as_ptr()];
    let log_wide = asked.log_dir.as_deref().map(wide);
    // Frame generation presents from the line's thread, in order with the
    // submissions: without the line it is not loaded at all.
    let framegen_asked = asked.framegen && (!require_line || line_asked());
    if asked.framegen && !framegen_asked {
        tracing::warn!(
            "DLSS Frame Generation needs the line of submissions (VILOCITY_THIN=1 or VILOCITY_ASYNC_SUBMIT=1): frame generation stays off"
        );
    }
    let mut features = Vec::new();
    if asked.dlss {
        features.push(abi::FEATURE_DLSS);
    }
    if framegen_asked {
        features.extend([abi::FEATURE_DLSS_G, abi::FEATURE_REFLEX, abi::FEATURE_PCL]);
    }
    let preferences = abi::Preferences {
        base: abi::BaseStructure::new(abi::PREFERENCES, 1),
        show_console: false,
        log_level: if asked.log_dir.is_some() { abi::LOG_VERBOSE } else { abi::LOG_DEFAULT },
        paths_to_plugins: paths.as_ptr(),
        num_paths_to_plugins: 1,
        path_to_logs_and_data: log_wide.as_ref().map_or(std::ptr::null(), |w| w.as_ptr()),
        allocate_callback: std::ptr::null(),
        release_callback: std::ptr::null(),
        log_message_callback: Some(log_message),
        // No downloads (OTA): only what is in the folder runs.
        flags: abi::DISABLE_CL_STATE_TRACKING
            | abi::DISABLE_DEBUG_TEXT
            | abi::USE_MANUAL_HOOKING
            | abi::USE_DXGI_FACTORY_PROXY
            | abi::USE_FRAME_BASED_RESOURCE_TAGGING,
        features_to_load: features.as_ptr(),
        num_features_to_load: features.len() as u32,
        application_id: 0,
        engine: abi::ENGINE_CUSTOM,
        engine_version: ENGINE_VERSION.as_ptr(),
        project_id: PROJECT_ID.as_ptr(),
        render_api: abi::RENDER_API_D3D12,
    };
    // SAFETY: a filled preferences structure; the library copies what it keeps.
    let code = unsafe { (sl.init)(&preferences, abi::SDK_VERSION) };
    if code != 0 {
        return Err(format!("slInit failed: {} ({code})", abi::result_name(code)));
    }
    let adapter = default_adapter();
    let luid = adapter.as_ref().map(|a| a.1);
    let supported = |feature: u32| -> Result<(), String> {
        let Some(luid) = luid else { return Err("no adapter".into()) };
        let info = abi::AdapterInfo {
            base: abi::BaseStructure::new(abi::ADAPTER_INFO, 1),
            device_luid: std::ptr::from_ref(&luid).cast(),
            device_luid_size: size_of_val(&luid) as u32,
            vk_physical_device: std::ptr::null_mut(),
        };
        // SAFETY: an adapter description that lives through the call.
        let code = unsafe { (sl.is_feature_supported)(feature, &info) };
        if code == 0 { Ok(()) } else { Err(format!("{} ({code})", abi::result_name(code))) }
    };
    let dlss = asked.dlss && report("DLSS Super Resolution", abi::FEATURE_DLSS, supported(abi::FEATURE_DLSS), sl);
    let framegen = framegen_asked
        && report("DLSS Frame Generation", abi::FEATURE_DLSS_G, supported(abi::FEATURE_DLSS_G), sl)
        && report("Reflex", abi::FEATURE_REFLEX, supported(abi::FEATURE_REFLEX), sl);
    tracing::info!(
        library = %path.display(),
        sdk = "2.14.16",
        adapter = adapter.as_ref().map_or("?", |a| a.0.as_str()),
        dlss,
        framegen,
        mode = asked.mode.name(),
        preset = asked.preset,
        "Streamline loaded (manual hooking, no downloads)"
    );
    let _ = PLAN.set(Plan { asked: asked.clone(), dlss, framegen });
    Ok(Available { dlss, framegen })
}

/// Logs whether a feature is supported, with its requirements; whether it is.
fn report(name: &str, feature: u32, supported: Result<(), String>, sl: &Sl) -> bool {
    let mut requirements: abi::FeatureRequirements = unsafe_zeroed_requirements();
    // SAFETY: a structure to fill.
    let asked = unsafe { (sl.get_feature_requirements)(feature, &mut requirements) };
    let needs = (asked == 0).then(|| {
        format!(
            "OS {} (needs {}), driver {} (needs {}){}{}",
            requirements.os_version_detected,
            requirements.os_version_required,
            requirements.driver_version_detected,
            requirements.driver_version_required,
            if requirements.flags & abi::REQUIRES_HARDWARE_SCHEDULING != 0 { ", hardware GPU scheduling" } else { "" },
            if requirements.flags & abi::REQUIRES_VSYNC_OFF != 0 { ", v-sync off" } else { "" },
        )
    });
    match supported {
        Ok(()) => {
            tracing::info!(requirements = needs.as_deref().unwrap_or("?"), "Streamline: {name} is supported on this GPU");
            true
        }
        Err(why) => {
            tracing::warn!(requirements = needs.as_deref().unwrap_or("?"), "Streamline: {name} is not supported here: {why}");
            false
        }
    }
}

fn unsafe_zeroed_requirements() -> abi::FeatureRequirements {
    // SAFETY: plain data; pointers null, counts zero.
    let mut r: abi::FeatureRequirements = unsafe { std::mem::zeroed() };
    r.base = abi::BaseStructure::new(abi::FEATURE_REQUIREMENTS, 2);
    r
}

/// Whether the switches turn the line of submissions on (frame generation
/// presents from its thread, in order with the submissions).
fn line_asked() -> bool {
    let on = |name: &str| std::env::var_os(name).is_some_and(|v| v == "1");
    on("VILOCITY_THIN") || on("VILOCITY_ASYNC_SUBMIT")
}

unsafe extern "C" fn log_message(kind: u32, message: *const c_char) {
    if message.is_null() {
        return;
    }
    // SAFETY: a NUL-terminated message for the call.
    let text = unsafe { CStr::from_ptr(message) }.to_string_lossy();
    let text = text.trim_end();
    // (Every plugin in the folder that was not asked for says so.)
    let noise = text.contains("since it is was not requested by the host");
    match kind {
        abi::LOG_ERROR => tracing::error!(target: "streamline", "{text}"),
        abi::LOG_WARN if !noise => tracing::warn!(target: "streamline", "{text}"),
        _ => tracing::debug!(target: "streamline", "{text}"),
    }
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain([0]).collect()
}

/// The adapter the run's device is made on, its name and LUID: the first
/// hardware one Windows counts as high performance, as
/// `vilocity_shader::emit::host::create_preferred_device` picks it (on a
/// laptop with two cards the first of all is often the one built into the
/// processor), else the first.
// shortcut: a card named in VILOCITY_D3D12_ADAPTER is not looked at here; upgrade if a PC needs both that and DLSS.
fn default_adapter() -> Option<(String, windows::Win32::Foundation::LUID)> {
    // SAFETY: plain DXGI queries.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let fast = factory.cast::<IDXGIFactory6>().ok().and_then(|f| {
            (0..8)
                .map_while(|i| f.EnumAdapterByGpuPreference::<IDXGIAdapter1>(i, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE).ok())
                .find(|a| a.GetDesc1().is_ok_and(|d| d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0))
        });
        let adapter = match fast {
            Some(adapter) => adapter,
            None => factory.EnumAdapters1(0).ok()?,
        };
        let desc = adapter.GetDesc1().ok()?;
        let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        Some((String::from_utf16_lossy(&desc.Description[..len]), desc.AdapterLuid))
    }
}

/// The Authenticode signature of `path` (NVIDIA signs its DLLs), checked
/// without going online.
///
/// (Vilocity) `WinVerifyTrust` is looked up here, in System32's
/// `wintrust.dll`, when a library is about to be loaded: named as an import
/// of the program (as foxhound has it), `wintrust.dll` and what it needs
/// would be loaded at the start of every run, with or without DLSS.
fn verify_signature(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::Security::WinTrust::{
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL,
        WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_NONE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
    /// `WinVerifyTrust` (`wintrust.h`): a window, the action's GUID, the data.
    type Verify = unsafe extern "system" fn(*mut c_void, *mut windows_sys::core::GUID, *mut c_void) -> i32;
    let library: Vec<u16> = "wintrust.dll".encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated name, looked for in System32 alone.
    let module = unsafe { LoadLibraryExW(library.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        return Err(format!("wintrust.dll did not load to check the signature ({})", std::io::Error::last_os_error()));
    }
    // SAFETY: a loaded module and a NUL-terminated name.
    let Some(found) = (unsafe { GetProcAddress(module, c"WinVerifyTrust".as_ptr().cast()) }) else {
        return Err("wintrust.dll has no WinVerifyTrust to check the signature".into());
    };
    // SAFETY: the export of that name has that signature.
    let verify = unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, Verify>(found) };
    let file = wide(path);
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: file.as_ptr(),
        hFile: std::ptr::null_mut(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut info },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: filled structures that live through both calls.
    let code = unsafe { verify(std::ptr::null_mut(), &mut action, (&raw mut data).cast()) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: as above; closes the state the first call opened.
    unsafe { verify(std::ptr::null_mut(), &mut action, (&raw mut data).cast()) };
    if code == 0 {
        Ok(())
    } else {
        Err(format!("{} is not validly signed ({:#x})", path.display(), code as u32))
    }
}

fn load(path: &Path) -> Result<Sl, String> {
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    let file = wide(path);
    // Its own folder and System32 only, never the current folder.
    // SAFETY: a NUL-terminated full path.
    let module =
        unsafe { LoadLibraryExW(file.as_ptr(), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if module.is_null() {
        return Err(format!("{INTERPOSER} did not load ({})", std::io::Error::last_os_error()));
    }
    let library = Library(module);
    let find = |name: &CStr| {
        // SAFETY: a loaded module and a NUL-terminated name.
        unsafe { GetProcAddress(module, name.as_ptr().cast()) }
            .map(|f| f as *const c_void)
            .ok_or_else(|| format!("{INTERPOSER} has no {}", name.to_string_lossy()))
    };
    macro_rules! get {
        ($name:literal, $t:ty) => {
            // SAFETY: the export of that name has the header's signature.
            unsafe { std::mem::transmute::<*const c_void, $t>(find($name)?) }
        };
    }
    Ok(Sl {
        init: get!(c"slInit", abi::SlInit),
        is_feature_supported: get!(c"slIsFeatureSupported", abi::SlIsFeatureSupported),
        evaluate_feature: get!(c"slEvaluateFeature", abi::SlEvaluateFeature),
        free_resources: get!(c"slFreeResources", abi::SlFreeResources),
        set_tag_for_frame: get!(c"slSetTagForFrame", abi::SlSetTagForFrame),
        get_feature_requirements: get!(c"slGetFeatureRequirements", abi::SlGetFeatureRequirements),
        get_feature_version: get!(c"slGetFeatureVersion", abi::SlGetFeatureVersion),
        upgrade_interface: get!(c"slUpgradeInterface", abi::SlUpgradeInterface),
        set_constants: get!(c"slSetConstants", abi::SlSetConstants),
        get_native_interface: get!(c"slGetNativeInterface", abi::SlGetNativeInterface),
        get_feature_function: get!(c"slGetFeatureFunction", abi::SlGetFeatureFunction),
        get_new_frame_token: get!(c"slGetNewFrameToken", abi::SlGetNewFrameToken),
        set_d3d_device: get!(c"slSetD3DDevice", abi::SlSetD3DDevice),
        shutdown: get!(c"slShutdown", abi::SlShutdown),
        _library: library,
    })
}

/// Whether the title's FidelityFX calls are to be served (DLSS or frame
/// generation is on): `vilocity_hle` then puts its own in front of them.
pub fn serving() -> bool {
    PLAN.get().is_some_and(|p| p.dlss || p.framegen)
}

/// Whether DLSS is to take the title's upscale dispatches (`VILOCITY_DLSS=1`
/// and Streamline loaded).
pub fn upscaling() -> bool {
    PLAN.get().is_some_and(|p| p.dlss)
}

/// (Vilocity) Whether Streamline is loaded with frame generation: only then
/// does a list note an upscaling or a copy for the present, and only then
/// does a submission look for one.
pub(crate) fn framegen_planned() -> bool {
    PLAN.get().is_some_and(|p| p.framegen)
}

/// The host device is made: Streamline is given it (`slSetD3DDevice`), the
/// feature functions are fetched, and, for frame generation, the direct
/// queue is made through Streamline's device proxy. The native queue to use
/// for everything, if it was made here.
pub(crate) fn device_made(device: &ID3D12Device) -> Option<ID3D12CommandQueue> {
    let plan = PLAN.get()?;
    // (Vilocity) Loaded, but neither feature is supported here: Streamline
    // is not given the device, and the run goes on as without it.
    if !plan.dlss && !plan.framegen {
        return None;
    }
    let sl = SL.get()?;
    // SAFETY: the native device.
    let code = unsafe { (sl.set_d3d_device)(device.as_raw()) };
    if code != 0 {
        tracing::warn!(
            "Streamline: slSetD3DDevice failed: {} ({code}); DLSS and frame generation stay off",
            abi::result_name(code)
        );
        return None;
    }
    let function = |feature: u32, name: &CStr| -> Option<*mut c_void> {
        let mut f = std::ptr::null_mut();
        // SAFETY: a NUL-terminated name and an out pointer.
        let code = unsafe { (sl.get_feature_function)(feature, name.as_ptr(), &mut f) };
        if code != 0 || f.is_null() {
            tracing::warn!(
                "Streamline: no {} ({}, {code})",
                name.to_string_lossy(),
                abi::result_name(code)
            );
            return None;
        }
        Some(f)
    };
    macro_rules! f {
        ($feature:expr, $name:literal, $t:ty) => {
            // SAFETY: the function of that name has the header's signature.
            function($feature, $name).map(|f| unsafe { std::mem::transmute::<*mut c_void, $t>(f) })
        };
    }
    let mut functions = Functions::default();
    if plan.dlss {
        functions.dlss_optimal = f!(abi::FEATURE_DLSS, c"slDLSSGetOptimalSettings", abi::SlDlssGetOptimalSettings);
        functions.dlss_set_options = f!(abi::FEATURE_DLSS, c"slDLSSSetOptions", abi::SlDlssSetOptions);
        let ready = functions.dlss_optimal.is_some() && functions.dlss_set_options.is_some();
        log_version(sl, abi::FEATURE_DLSS, "DLSS Super Resolution");
        DLSS_READY.store(ready, Release);
    }
    let mut queue = None;
    if plan.framegen {
        functions.dlssg_set_options = f!(abi::FEATURE_DLSS_G, c"slDLSSGSetOptions", abi::SlDlssgSetOptions);
        functions.dlssg_get_state = f!(abi::FEATURE_DLSS_G, c"slDLSSGGetState", abi::SlDlssgGetState);
        functions.reflex_set_options = f!(abi::FEATURE_REFLEX, c"slReflexSetOptions", abi::SlReflexSetOptions);
        functions.pcl_set_marker = f!(abi::FEATURE_PCL, c"slPCLSetMarker", abi::SlPclSetMarker);
        log_version(sl, abi::FEATURE_DLSS_G, "DLSS Frame Generation");
        log_version(sl, abi::FEATURE_REFLEX, "Reflex");
        let have = functions.dlssg_set_options.is_some()
            && functions.dlssg_get_state.is_some()
            && functions.reflex_set_options.is_some()
            && functions.pcl_set_marker.is_some();
        if have {
            if let Some(reflex) = functions.reflex_set_options {
                let options = abi::ReflexOptions {
                    base: abi::BaseStructure::new(abi::REFLEX_OPTIONS, 1),
                    mode: abi::REFLEX_LOW_LATENCY,
                    frame_limit_us: 0,
                    use_markers_to_optimize: false,
                    virtual_key: 0,
                    id_thread: 0,
                };
                // SAFETY: filled options.
                let code = unsafe { reflex(&options) };
                tracing::info!(code, result = abi::result_name(code), "Streamline: Reflex low latency on (needed by frame generation)");
            }
            queue = proxied_queue(sl, device);
        }
        FG_READY.store(have && queue.is_some(), Release);
        if !(have && queue.is_some()) {
            tracing::warn!("DLSS Frame Generation stays off: the device hand-over to Streamline did not complete");
        }
    }
    let _ = FUNCTIONS.set(functions);
    queue
}

fn log_version(sl: &Sl, feature: u32, name: &str) {
    let mut version = abi::FeatureVersion {
        base: abi::BaseStructure::new(abi::FEATURE_VERSION, 1),
        version_sl: abi::Version::default(),
        version_ngx: abi::Version::default(),
    };
    // SAFETY: a structure to fill.
    let code = unsafe { (sl.get_feature_version)(feature, &mut version) };
    if code == 0 {
        tracing::info!(streamline = %version.version_sl, ngx = %version.version_ngx, "Streamline: {name} loaded");
    } else {
        tracing::warn!("Streamline: {name} has no version: {} ({code})", abi::result_name(code));
    }
}

/// The direct queue, made through Streamline's proxy of `device` (frame
/// generation hooks queue creation); the native queue is returned for our
/// use and the proxy kept for the swap chain.
fn proxied_queue(sl: &Sl, device: &ID3D12Device) -> Option<ID3D12CommandQueue> {
    let mut raw = device.as_raw();
    // SAFETY: an owned reference of ours; on success `raw` is a new proxy
    // that holds its own reference to the device.
    let code = unsafe { (sl.upgrade_interface)(&mut raw) };
    if code != 0 || raw.is_null() || raw == device.as_raw() {
        tracing::warn!("Streamline: the device was not upgraded: {} ({code})", abi::result_name(code));
        return None;
    }
    // SAFETY: the proxy's one reference, now ours.
    let proxy_device = unsafe { ID3D12Device::from_raw(raw) };
    let desc = D3D12_COMMAND_QUEUE_DESC { Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default() };
    // SAFETY: plain queue creation through the proxy.
    let proxy_queue: ID3D12CommandQueue = match unsafe { proxy_device.CreateCommandQueue(&desc) } {
        Ok(q) => q,
        Err(e) => {
            tracing::warn!(error = %e, "Streamline: no queue from the device proxy");
            return None;
        }
    };
    let mut native = std::ptr::null_mut();
    // SAFETY: a proxy queue; the native one comes back with a reference.
    let code = unsafe { (sl.get_native_interface)(proxy_queue.as_raw(), &mut native) };
    if code != 0 || native.is_null() {
        tracing::warn!("Streamline: no native queue behind the proxy: {} ({code})", abi::result_name(code));
        return None;
    }
    // SAFETY: the reference slGetNativeInterface added.
    let native = unsafe { ID3D12CommandQueue::from_raw(native) };
    tracing::info!("Streamline: the direct queue was made through the device proxy (frame generation)");
    let _ = PROXIES.set(Proxies { _device: proxy_device, queue: proxy_queue });
    Some(native)
}

/// For frame generation: Streamline's proxy of `factory` and the queue
/// proxy to make the swap chain with; `None` to make it as before.
pub(crate) fn swap_chain_factory(factory: &IDXGIFactory2) -> Option<(IDXGIFactory2, ID3D12CommandQueue)> {
    if !FG_READY.load(Acquire) {
        return None;
    }
    let sl = SL.get()?;
    let queue = PROXIES.get()?.queue.clone();
    let mut raw = factory.as_raw();
    // SAFETY: as for the device: a new proxy with its own reference.
    let code = unsafe { (sl.upgrade_interface)(&mut raw) };
    if code != 0 || raw.is_null() || raw == factory.as_raw() {
        tracing::warn!("Streamline: the DXGI factory was not upgraded: {} ({code})", abi::result_name(code));
        FG_READY.store(false, Release);
        return None;
    }
    // SAFETY: the proxy's reference, now ours.
    let proxy = unsafe { IDXGIFactory2::from_raw(raw) };
    Some((proxy, queue))
}

/// The swap chain was made through the proxy (or not, after all: then
/// frame generation stays off).
pub(crate) fn swap_chain_made(proxied: bool, error: Option<&windows::core::Error>) {
    SWAP_CHAIN_PROXIED.store(proxied, Release);
    HUDLESS_WANTED.store(proxied && FG_READY.load(Acquire) && PLAN.get().is_some_and(|p| p.asked.hudless), Release);
    if proxied {
        tracing::info!("Streamline: our swap chain is Streamline's proxy; DLSS Frame Generation can run on its presents");
    } else {
        FG_READY.store(false, Release);
        tracing::warn!(
            error = error.map(|e| e.to_string()).unwrap_or_default(),
            "Streamline: the swap chain could not be made through the proxy; made as before, frame generation off"
        );
    }
}

/// `VILOCITY_STREAMLINE_PRESENT=1`, DLSS without frame generation: Streamline's
/// proxy of `swap_chain`, whose Present, ResizeBuffers and GetBuffer run
/// Streamline's hooks first (its common plugin frees what it retired and
/// keeps its video memory budget at each present; without it, it logs
/// "presentCommon() was not observed" after 60 upscalings and frees nothing
/// it retired). `swap_chain` itself if the switch is off, frame generation
/// owns the swap chain, or the upgrade fails.
pub(crate) fn present_hooked(swap_chain: IDXGISwapChain3) -> IDXGISwapChain3 {
    let (Some(plan), Some(sl)) = (PLAN.get(), SL.get()) else { return swap_chain };
    if !plan.asked.present_hook || !plan.dlss || !DLSS_READY.load(Acquire) || SWAP_CHAIN_PROXIED.load(Acquire) {
        return swap_chain;
    }
    let mut raw = swap_chain.as_raw();
    // SAFETY: an owned reference of ours; on success `raw` is a new proxy
    // that holds its own reference to the swap chain.
    let code = unsafe { (sl.upgrade_interface)(&mut raw) };
    if code != 0 || raw.is_null() || raw == swap_chain.as_raw() {
        tracing::warn!(
            "Streamline: the swap chain was not upgraded: {} ({code}); presents stay unseen by Streamline",
            abi::result_name(code)
        );
        return swap_chain;
    }
    tracing::info!("Streamline: our swap chain is Streamline's proxy of it (VILOCITY_STREAMLINE_PRESENT=1): it sees every present");
    PRESENT_HOOKED.store(true, Release);
    // SAFETY: the proxy's one reference, now ours; it implements the swap
    // chain interfaces up to IDXGISwapChain4 and holds the swap chain.
    unsafe { IDXGISwapChain3::from_raw(raw) }
}

fn frame_token(sl: &Sl, index: u32) -> Option<*mut abi::FrameToken> {
    let mut token = std::ptr::null_mut();
    // SAFETY: an out pointer and an index.
    let code = unsafe { (sl.get_new_frame_token)(&mut token, &index) };
    (code == 0 && !token.is_null()).then_some(token)
}

/// The upscaling `up` asked for on the title's thread: DLSS records it at
/// this place of the title's list instead (or, with frame generation alone,
/// its inputs are noted there for the present); the title's own runs
/// otherwise.
///
/// (Vilocity) foxhound puts the work into the title's list
/// (`list::external`), to run where the list is translated, and sets all
/// the title's draw state again after it. This tree's lists take a
/// library's work at the call (`crate::external::record`, as the PC
/// FidelityFX DLL's: the title's thread waits for a list written down to be
/// translated that far), so the upscaling is recorded here and now; the
/// draw state is set again as foxhound does (`crate::external::Pass::all_again`).
pub fn upscale(up: &Upscale) -> Taken {
    let Some(plan) = PLAN.get() else {
        return Taken::No("Streamline is not loaded");
    };
    let dlss = plan.dlss && DLSS_READY.load(Acquire);
    let framegen = plan.framegen && FG_READY.load(Acquire);
    if !dlss && !framegen {
        return Taken::No("Streamline has no device yet, or its features did not start");
    }
    if [COLOUR, DEPTH, MOTION, OUTPUT].iter().any(|&i| up.resources[i].0.is_null()) {
        return Taken::No("a required texture (colour, depth, motion vectors, output) is missing");
    }
    if up.camera.render.contains(&0) || up.camera.output.contains(&0) {
        return Taken::No("an empty render or output size");
    }
    if dlss && let Err(why) = validate(up) {
        return Taken::No(why);
    }
    let id = NEXT_ID.fetch_add(1, Relaxed) + 1;
    let asked = crate::external::record(up.list.cast(), |pass| {
        if framegen {
            note(pass.host_list(), up, id);
        }
        if dlss && evaluate(pass, up, id) {
            pass.all_again();
        }
    });
    if !asked {
        return Taken::No("the title's list records nowhere (no host list)");
    }
    if dlss { Taken::Dlss } else { Taken::Noted }
}

/// The title's own upscaling ran for this reason.
pub fn forwarded() {
    FORWARDED.fetch_add(1, Relaxed);
}

/// The host state DLSS is told a texture of an upscaling is in: the one
/// `crate::external::Pass::ready` puts it in, whatever state the title names
/// for its console texture. (Vilocity; foxhound moves each to the state the
/// title gave FidelityFX.)
fn told_state(i: usize) -> u32 {
    if i == OUTPUT { crate::external::WRITE } else { crate::external::READ }
}

/// What an upscaling's textures must be for DLSS, checked on the title's
/// thread before anything is recorded (the title's own runs if not): live
/// textures of the title's with host copies, in states they can be in.
fn validate(up: &Upscale) -> Result<(), &'static str> {
    for (i, &(resource, _)) in up.resources.iter().enumerate() {
        if resource.is_null() || i == 5 {
            continue;
        }
        if !crate::resource::is_texture(resource) {
            return Err("a texture of the upscaling is no live texture of the title's");
        }
        let Some(copy) = crate::resource::host_copy(resource) else {
            return Err("a texture of the upscaling has no host copy");
        };
        let wanted = told_state(i);
        if crate::host::legal_state(&copy.texture, wanted) != wanted {
            return Err(if i == OUTPUT {
                "the output cannot be written by a shader (no unordered access)"
            } else {
                "an input cannot be read by a shader"
            });
        }
    }
    Ok(())
}

/// Frame generation: what the present after this list's submission tags,
/// noted on the host list ([`submitted`] then makes it the latest).
fn note(host: &crate::host::HostList, up: &Upscale, id: u64) {
    let (Some(depth), Some(motion)) =
        (crate::resource::host_copy(up.resources[DEPTH].0), crate::resource::host_copy(up.resources[MOTION].0))
    else {
        return;
    };
    let mut pending = pending();
    pending.retain(|n| id.saturating_sub(n.id) < 16);
    pending.push(Noted { id, depth: Source::Host(depth), motion: Source::Host(motion), camera: up.camera });
    drop(pending);
    host.mark_upscale(id);
}

/// The inputs of an upscaling for frame generation.
struct Noted {
    id: u64,
    depth: Source,
    motion: Source,
    camera: plan::Camera,
}

/// A texture frame generation reads at the present, and the state it is in
/// then: a title texture's host copy (in the state the lists submitted so
/// far left it in), or one of the check's ([`check`]) in a fixed state.
#[derive(Clone)]
enum Source {
    Host(Arc<crate::resource::HostTexture>),
    Fixed(ID3D12Resource, u32),
}

impl Source {
    fn tagged(&self) -> abi::Resource {
        match self {
            Source::Host(t) => abi::Resource::texture(t.texture.as_raw(), t.state.load(Relaxed)),
            Source::Fixed(t, state) => abi::Resource::texture(t.as_raw(), *state),
        }
    }
}

// SAFETY: D3D12 resources are free-threaded.
unsafe impl Send for Source {}

static PENDING: Mutex<Vec<Noted>> = Mutex::new(Vec::new());
/// The latest upscaling submitted, and the one the last present tagged.
static SUBMITTED: Mutex<Option<Noted>> = Mutex::new(None);
static PRESENTED_ID: AtomicU64 = AtomicU64::new(0);

fn pending() -> std::sync::MutexGuard<'static, Vec<Noted>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// A host list that recorded upscaling `id` is submitted (in submission
/// order, before the present that follows it).
pub(crate) fn submitted(id: u64) {
    let noted = {
        let mut pending = pending();
        let at = pending.iter().position(|n| n.id == id);
        at.map(|i| pending.swap_remove(i))
    };
    if let Some(noted) = noted {
        let mut latest = SUBMITTED.lock().unwrap_or_else(|e| e.into_inner());
        if latest.as_ref().is_none_or(|l| l.id < noted.id) {
            *latest = Some(noted);
        }
    }
}

/// DLSS in place of the title's upscaling, recorded onto `pass`'s list
/// (where the title's list is now). Whether it recorded anything.
fn evaluate(pass: &mut crate::external::Pass, up: &Upscale, id: u64) -> bool {
    let Some(plan) = PLAN.get() else { return false };
    let camera = up.camera;
    let fail = |why: &str| {
        FAILED.fetch_add(1, Relaxed);
        tracing::warn!(id, "DLSS: the upscaling was not recorded: {why} (the output keeps what it had)");
        false
    };
    // Each texture made ready as a PC library's is here (an input as a
    // shader's read: aliased views, uploads of title-written textures, depth
    // twins; the output as a shader's write), in the state DLSS is told.
    let used = |i: usize| {
        !up.resources[i].0.is_null()
            && (i != REACTIVE || plan.asked.reactive)
            && (i != EXPOSURE || camera.flags & plan::FFX_AUTO_EXPOSURE == 0)
    };
    let mut textures: Textures = Default::default();
    for i in [COLOUR, DEPTH, MOTION, EXPOSURE, REACTIVE, OUTPUT] {
        if !used(i) {
            continue;
        }
        let resource = up.resources[i].0;
        let wanted = told_state(i);
        let ready = pass.ready(resource.cast(), i == OUTPUT).and_then(|_| crate::resource::host_copy(resource));
        let Some(copy) = ready else {
            return fail("a texture has no host copy");
        };
        if crate::host::legal_state(&copy.texture, wanted) != wanted {
            return fail("a texture cannot be in the state DLSS needs");
        }
        textures[i] = Some((copy.texture.clone(), wanted));
    }
    let recorded = match record(pass.command_list(), &textures, &camera, up.pre_exposure, id) {
        Ok(recorded) => recorded,
        Err(why) => return fail(&why),
    };
    EVALUATED.fetch_add(1, Relaxed);
    let all = EVALUATED_ALL.fetch_add(1, Relaxed) + 1;
    if all == 1 || recorded.changed {
        tracing::info!(
            render = ?camera.render,
            output = ?camera.output,
            mode = plan::sl_mode_name(recorded.mode),
            asked = plan.asked.mode.name(),
            preset = plan.asked.preset,
            optimal = ?recorded.optimal,
            hdr = camera.flags & plan::FFX_HIGH_DYNAMIC_RANGE != 0,
            depth_inverted = camera.depth_inverted(),
            auto_exposure = textures[EXPOSURE].is_none(),
            reactive = textures[REACTIVE].is_some(),
            display_motion_vectors = camera.display_motion_vectors(),
            jitter = ?recorded.jitter,
            mvec_scale = ?recorded.mvec_scale,
            near = camera.near,
            far = camera.far,
            fov = camera.fov_y,
            formats = ?formats(&textures),
            evaluations = all,
            "DLSS: upscaling recorded in place of the title's FidelityFX"
        );
    }
    true
}

/// Colour, depth, motion vectors, exposure, reactive mask, (transparency),
/// output: the host textures DLSS is given and the states they are in.
type Textures = [Option<(ID3D12Resource, u32)>; 7];

fn formats(textures: &Textures) -> Vec<i32> {
    textures
        .iter()
        .flatten()
        .map(|(t, _)| {
            // SAFETY: a live texture.
            unsafe { t.GetDesc() }.Format.0
        })
        .collect()
}

/// What [`record`] recorded.
struct Recorded {
    mode: u32,
    optimal: Option<[u32; 2]>,
    /// Another mode or size than the last time.
    changed: bool,
    jitter: [f32; 2],
    mvec_scale: [f32; 2],
}

/// The DLSS evaluation of `textures` recorded into the open command list
/// `list`: the mode whose render size holds the title's, the options, the
/// frame's constants (frame `id` of the upscaling's viewport), the textures
/// as local tags. The textures are left in the states given.
fn record(list: *mut c_void, textures: &Textures, camera: &plan::Camera, pre_exposure: f32, id: u64) -> Result<Recorded, String> {
    let (Some(plan), Some(sl), Some(functions)) = (PLAN.get(), SL.get(), FUNCTIONS.get()) else {
        return Err("Streamline is not ready".into());
    };
    let (Some(optimal_of), Some(set_options)) = (functions.dlss_optimal, functions.dlss_set_options) else {
        return Err("no DLSS functions".into());
    };
    // (Timed with frame generation: its present shares this lock and
    // Streamline with the upscaling.)
    let began = plan.framegen.then(std::time::Instant::now);
    let mut calls = calls();
    if let Some(began) = began {
        spent(T_UPSCALE_LOCK, began);
    }
    let output = camera.output;
    let optimal = |calls: &mut Calls, mode: u32| -> Option<[u32; 2]> {
        if let Some(&(_, _, size)) = calls.optimal.iter().find(|(o, m, _)| *o == output && *m == mode) {
            return size;
        }
        let options = abi::DlssOptions::new(mode, output, plan.asked.preset);
        let mut settings = abi::DlssOptimalSettings::default();
        // SAFETY: filled options and a structure to fill.
        let code = unsafe { optimal_of(&options, &mut settings) };
        let size = (code == 0 && settings.optimal_render_width > 0)
            .then_some([settings.optimal_render_width, settings.optimal_render_height]);
        calls.optimal.push((output, mode, size));
        size
    };
    let sizes: Vec<(u32, Option<[u32; 2]>)> = plan::MODES.iter().map(|&m| (m, optimal(&mut calls, m))).collect();
    let optimal_of_mode = |m: u32| sizes.iter().find(|s| s.0 == m).and_then(|s| s.1);
    let mode = plan::pick_mode(camera.render, output, optimal_of_mode)
        .ok_or_else(|| format!("no DLSS mode takes a {:?} render for a {output:?} output", camera.render))?;
    let mut options = abi::DlssOptions::new(mode, output, plan.asked.preset);
    options.pre_exposure = if pre_exposure > 0.0 { pre_exposure } else { 1.0 };
    options.color_buffers_hdr = abi::boolean(camera.flags & plan::FFX_HIGH_DYNAMIC_RANGE != 0);
    options.use_auto_exposure = abi::boolean(textures[EXPOSURE].is_none());
    let viewport = abi::ViewportHandle::new(VIEWPORT_DLSS);
    // SAFETY: filled options for our viewport.
    let code = unsafe { set_options(&viewport, &options) };
    if code != 0 {
        return Err(format!("slDLSSSetOptions: {} ({code})", abi::result_name(code)));
    }
    let changed = calls.last != Some((mode, camera.render, output));
    calls.last = Some((mode, camera.render, output));
    let token = frame_token(sl, id as u32).ok_or("no frame token")?;
    let constants = camera.constants(plan.asked.jitter, plan.asked.mvec);
    // SAFETY: constants for this frame and viewport.
    let code = unsafe { (sl.set_constants)(&constants, token, &viewport) };
    if code != 0 {
        return Err(format!("slSetConstants: {} ({code})", abi::result_name(code)));
    }
    let kinds = [
        (COLOUR, abi::BUFFER_SCALING_INPUT_COLOR, camera.render),
        (DEPTH, abi::BUFFER_DEPTH, camera.render),
        (MOTION, abi::BUFFER_MOTION_VECTORS, camera.motion_size()),
        (EXPOSURE, abi::BUFFER_EXPOSURE, [1, 1]),
        (REACTIVE, abi::BUFFER_BIAS_CURRENT_COLOR_HINT, camera.render),
        (OUTPUT, abi::BUFFER_SCALING_OUTPUT_COLOR, output),
    ];
    let mut resources: Vec<(abi::Resource, u32, [u32; 2])> = kinds
        .iter()
        .filter_map(|&(i, kind, size)| {
            textures[i].as_ref().map(|(t, state)| (abi::Resource::texture(t.as_raw(), *state), kind, size))
        })
        .collect();
    let mut tags: Vec<abi::ResourceTag> = resources
        .iter_mut()
        .map(|(r, kind, size)| abi::ResourceTag::new(r, *kind, abi::VALID_UNTIL_EVALUATE, abi::Extent::sized(*size)))
        .collect();
    let mut inputs: Vec<*const abi::BaseStructure> = vec![std::ptr::from_ref(&viewport.base)];
    inputs.extend(tags.iter_mut().map(|t| std::ptr::from_ref(&t.base)));
    // SAFETY: an open command list; inputs that live through the call.
    let code = unsafe { (sl.evaluate_feature)(abi::FEATURE_DLSS, token, inputs.as_ptr(), inputs.len() as u32, list) };
    if code != 0 {
        return Err(format!("slEvaluateFeature: {} ({code})", abi::result_name(code)));
    }
    if let Some(began) = began {
        spent(T_UPSCALE, began);
    }
    Ok(Recorded { mode, optimal: optimal_of_mode(mode), changed, jitter: constants.jitter_offset, mvec_scale: constants.mvec_scale })
}

/// The title's FidelityFX upscaler `context` is destroyed: what DLSS made
/// for it goes once submitted work has run.
pub fn upscaler_destroyed(context: usize) {
    let (Some(sl), true) = (SL.get(), DLSS_READY.load(Acquire)) else { return };
    let _ = context;
    // What the title submitted is sent (the line of submissions first) and
    // settled, before DLSS lets go of resources the GPU may still use.
    crate::queue::before_sync();
    crate::host::settle_for_fence();
    let viewport = abi::ViewportHandle::new(VIEWPORT_DLSS);
    let mut calls = calls();
    // SAFETY: our viewport; nothing that names it is in flight.
    let code = unsafe { (sl.free_resources)(abi::FEATURE_DLSS, &viewport) };
    calls.last = None;
    tracing::info!(code, result = abi::result_name(code), "DLSS: the title's upscaler was destroyed; DLSS let go of its resources");
}

/// The title's interface composite where the interface is no plane of the
/// present (Series X): one triangle that blends the interface target
/// (3840x2160, premultiplied) over the frame's colour, with these vertex and
/// pixel shaders, the frame's last draw into the texture it presents. Seen
/// in every frame listing of 2026-11-26 (stock 3840x2160 output: once a
/// frame, after the scene's composite into the same texture) and bound from
/// frame 3 on in the 1440p runs with DLSS. (foxhound's runs, with its engine.)
const INTERFACE_COMPOSITE: [&str; 2] = ["0602c2384acbb3b2", "a78d088fa55aa495"];

/// Whether draws are looked at for the colour without the interface: frame
/// generation runs on our swap chain and `VILOCITY_FRAMEGEN_HUDLESS` is not 0.
#[inline]
pub(crate) fn hudless_wanted() -> bool {
    HUDLESS_WANTED.load(Relaxed)
}

static HUDLESS_WANTED: AtomicBool = AtomicBool::new(false);

/// The host textures of the last presents (their addresses): the textures
/// the title presents (it has two, in turn).
static SHOWN: [std::sync::atomic::AtomicUsize; 4] = [const { std::sync::atomic::AtomicUsize::new(0) }; 4];
static SHOWN_NEXT: AtomicU32 = AtomicU32::new(0);

/// A copy of a presented texture taken in front of the interface composite.
struct Copied {
    id: u64,
    /// The host texture copied (its address).
    target: usize,
    texture: ID3D12Resource,
}

// SAFETY: D3D12 resources are free-threaded.
unsafe impl Send for Copied {}

/// The copies recorded in lists not submitted yet, and those submitted and
/// not presented yet, in submission order.
static HUD_PENDING: Mutex<Vec<Copied>> = Mutex::new(Vec::new());
static HUD_SUBMITTED: Mutex<Vec<Copied>> = Mutex::new(Vec::new());
static HUD_NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// The two textures the copies go to in turn (a present reads one while the
/// next frame's lists may already be recorded), with their size and format.
struct HudlessTextures {
    size: [u32; 2],
    format: i32,
    textures: [ID3D12Resource; 2],
}

// SAFETY: as above.
unsafe impl Send for HudlessTextures {}

static HUD_TEXTURES: Mutex<Option<HudlessTextures>> = Mutex::new(None);

// Since the last line about it (every ten seconds): copies recorded, presents
// tagged with one, presents with frame generation on that had none, composite
// draws, those after a copy in their list, draws of other pipelines into a
// presented texture.
static HUD_COUNTS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
const HUD_COPIES: usize = 0;
const HUD_TAGGED: usize = 1;
const HUD_MISSING: usize = 2;
const HUD_COMPOSITES: usize = 3;
const HUD_AGAIN: usize = 4;
const HUD_OTHERS: usize = 5;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Before a draw of `pipeline` into `target` alone (no depth), with frame
/// generation: if `target` is a texture the title presents and the draw is
/// the title's interface composite ([`INTERFACE_COMPOSITE`]; `triangle`: one
/// triangle, one instance), the target as it is now, the frame without the
/// interface, is copied into a texture of ours, recorded into `list` in
/// front of the draw; the present after the list's submission tags it as
/// the colour without the interface. One copy a list. Other pipelines that
/// draw into a presented texture are named in the log once each: the
/// interface is in the copy if one of them draws it before the composite.
pub(crate) fn before_target_draw(
    list: &crate::host::HostList,
    target: crate::object::This,
    pipeline: &crate::pipeline::host::HostPipeline,
    triangle: bool,
) {
    let Some(texture) = crate::resource::host_resource(target) else { return };
    let shown = texture.as_raw() as usize;
    if !SHOWN.iter().any(|s| s.load(Relaxed) == shown) {
        return;
    }
    let composite = triangle && INTERFACE_COMPOSITE.iter().all(|hash| pipeline.stages.iter().any(|s| s.shader.hash == *hash));
    if !composite {
        HUD_COUNTS[HUD_OTHERS].fetch_add(1, Relaxed);
        static SEEN: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        let key = std::ptr::from_ref(pipeline) as usize;
        let mut seen = lock(&SEEN);
        if !seen.contains(&key) && seen.len() < 24 {
            seen.push(key);
            tracing::info!(
                pipeline = %pipeline.name,
                state = %pipeline.state,
                triangle,
                after_the_copy_in_its_list = list.hudless() != 0,
                "DLSS Frame Generation: this pipeline draws into the texture the title presents (the scene's composite does; an interface drawn here before the interface composite would be in the colour without the interface)"
            );
        }
        return;
    }
    HUD_COUNTS[HUD_COMPOSITES].fetch_add(1, Relaxed);
    if list.hudless() != 0 {
        HUD_COUNTS[HUD_AGAIN].fetch_add(1, Relaxed);
        return;
    }
    let began = std::time::Instant::now();
    let id = HUD_NEXT_ID.fetch_add(1, Relaxed) + 1;
    let Some(copy) = hudless_texture(texture, id) else { return };
    // The target as a copy's source: the composite's own preparation makes
    // it a render target again.
    const COPY_SOURCE: u32 = 0x800;
    crate::list::ensure_state(list, target, COPY_SOURCE);
    // SAFETY: an open list; the source is in the copy source state in it,
    // ours stays a copy's destination; same size, one level, formats of one
    // family.
    unsafe { list.list.CopyResource(&copy, texture) };
    list.keep(&copy);
    list.keep(texture);
    let mut pending = lock(&HUD_PENDING);
    pending.retain(|c| id.saturating_sub(c.id) < 16);
    pending.push(Copied { id, target: shown, texture: copy });
    drop(pending);
    list.mark_hudless(id);
    HUD_COUNTS[HUD_COPIES].fetch_add(1, Relaxed);
    spent(T_COPY, began);
    // SAFETY: a live texture.
    let desc = unsafe { texture.GetDesc() };
    once!(
        info,
        width = desc.Width,
        height = desc.Height,
        format = desc.Format.0,
        pipeline = %pipeline.name,
        state = %pipeline.state,
        "DLSS Frame Generation: the colour without the interface is copied in front of the title's interface composite (first time)"
    );
}

/// One of the two textures the colour without the interface is copied to
/// (copy `id`'s), made like `target` in the swap chain's format; `None` if
/// `target` is no plain 2D texture of a format that is shown.
fn hudless_texture(target: &ID3D12Resource, id: u64) -> Option<ID3D12Resource> {
    // SAFETY: a live texture.
    let desc = unsafe { target.GetDesc() };
    let plain = desc.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE2D
        && desc.DepthOrArraySize == 1
        && desc.MipLevels == 1
        && desc.SampleDesc.Count == 1;
    let format = crate::host::swap_format(desc.Format).filter(|_| plain);
    let Some(format) = format else {
        once!(
            warn,
            format = desc.Format.0,
            levels = desc.MipLevels,
            samples = desc.SampleDesc.Count,
            "DLSS Frame Generation: the presented texture cannot be copied for the colour without the interface; none is tagged"
        );
        return None;
    };
    let size = [desc.Width as u32, desc.Height];
    let mut textures = lock(&HUD_TEXTURES);
    if textures.as_ref().is_none_or(|t| t.size != size || t.format != format.0) {
        let device = &crate::host::host()?.device;
        let make = || texture(device, size, format, D3D12_RESOURCE_FLAG_NONE, COPY_DEST);
        match (make(), make()) {
            (Ok(a), Ok(b)) => *textures = Some(HudlessTextures { size, format: format.0, textures: [a, b] }),
            (Err(e), _) | (_, Err(e)) => {
                once!(warn, error = %e, "DLSS Frame Generation: no texture for the colour without the interface; none is tagged");
                return None;
            }
        }
    }
    textures.as_ref().map(|t| t.textures[(id % 2) as usize].clone())
}

/// A host list that recorded copy `id` of the colour without the interface
/// is submitted (in submission order, before the present that follows it).
pub(crate) fn hudless_submitted(id: u64) {
    let copied = {
        let mut pending = lock(&HUD_PENDING);
        let at = pending.iter().position(|c| c.id == id);
        at.map(|i| pending.swap_remove(i))
    };
    if let Some(copied) = copied {
        let mut submitted = lock(&HUD_SUBMITTED);
        if submitted.len() >= 8 {
            submitted.remove(0);
        }
        submitted.push(copied);
    }
}

/// At a present of host texture `shown`: it is one the title presents
/// ([`SHOWN`]), and the first copy of it submitted since its last present,
/// if any, is the frame's colour without the interface.
fn hudless_for(shown: &ID3D12Resource) -> Option<ID3D12Resource> {
    let shown = shown.as_raw() as usize;
    if !SHOWN.iter().any(|s| s.load(Relaxed) == shown) {
        SHOWN[SHOWN_NEXT.fetch_add(1, Relaxed) as usize % SHOWN.len()].store(shown, Relaxed);
    }
    let mut submitted = lock(&HUD_SUBMITTED);
    let first = submitted.iter().position(|c| c.target == shown).map(|i| submitted.remove(i));
    submitted.retain(|c| c.target != shown);
    first.map(|c| c.texture)
}

/// The planes a present shows (Series S: the scene and the interface over
/// it), the host states they are in, and the back buffer's size.
pub(crate) struct Planes<'a> {
    pub base: (&'a ID3D12Resource, u32),
    pub overlay: Option<(&'a ID3D12Resource, u32)>,
    pub size: [u32; 2],
}

/// What [`present_begins`] began, for [`present_ends`].
pub(crate) struct Presenting {
    token: *mut abi::FrameToken,
    frame: u32,
}

/// Before our swap chain's Present of title frame `frame`: with frame
/// generation, the frame's latency markers, constants and tags, and whether
/// frame generation runs on it (it does when an upscaling was submitted
/// since the last present).
pub(crate) fn present_begins(frame: u64, planes: Planes<'_>) -> Option<Presenting> {
    if !FG_READY.load(Acquire) || !SWAP_CHAIN_PROXIED.load(Acquire) {
        return None;
    }
    // The tagged textures' states are those the submissions before this
    // present left: only the line of submissions orders the two.
    if !crate::queue::line_on() && !CHECKING.load(Relaxed) {
        HUDLESS_WANTED.store(false, Relaxed);
        once!(warn, "DLSS Frame Generation: the line of submissions did not start; frame generation stays off");
        return None;
    }
    // Where the interface is no plane of its own (Series X): the copy of
    // this texture taken in front of the title's interface composite.
    let hudless = if planes.overlay.is_none() && hudless_wanted() { hudless_for(planes.base.0) } else { None };
    let (sl, functions) = (SL.get()?, FUNCTIONS.get()?);
    let (set_options, marker) = (functions.dlssg_set_options?, functions.pcl_set_marker?);
    let began = std::time::Instant::now();
    let frame = frame as u32;
    let token = frame_token(sl, frame)?;
    let mark = |m: u32| {
        let began = std::time::Instant::now();
        // SAFETY: a marker for a token of ours.
        unsafe { marker(m, token) };
        spent(T_MARKERS, began);
    };
    let mut calls = calls();
    if calls.simulating != Some(frame) {
        mark(abi::SIMULATION_START);
    }
    calls.simulating = None;
    mark(abi::SIMULATION_END);
    mark(abi::RENDER_SUBMIT_START);
    mark(abi::RENDER_SUBMIT_END);
    let fresh = {
        let latest = SUBMITTED.lock().unwrap_or_else(|e| e.into_inner());
        latest
            .as_ref()
            .filter(|n| n.id > PRESENTED_ID.load(Relaxed))
            .map(|n| (n.id, n.depth.clone(), n.motion.clone(), n.camera))
    };
    let viewport = abi::ViewportHandle::new(VIEWPORT_FG);
    let on = if let Some((id, depth, motion, camera)) = fresh {
        PRESENTED_ID.store(id, Relaxed);
        let plan = PLAN.get()?;
        // (Frame generation reads the camera's planes and matrices: the
        // nearer plane is the near one, whichever way the title names them.)
        let camera = camera.planes_in_order();
        let constants = camera.constants(plan.asked.jitter, plan.asked.mvec);
        // SAFETY: constants for this frame and viewport.
        let code = unsafe { (sl.set_constants)(&constants, token, &viewport) };
        if code != 0 {
            once!(warn, "frame generation: slSetConstants: {} ({code})", abi::result_name(code));
        }
        let mut resources = vec![
            (depth.tagged(), abi::BUFFER_DEPTH, camera.render),
            (motion.tagged(), abi::BUFFER_MOTION_VECTORS, camera.motion_size()),
        ];
        // Two planes: the scene is the colour without the interface, the
        // interface plane its colour and alpha (premultiplied, as `compose`
        // blends it).
        if let Some((over, over_state)) = planes.overlay {
            resources.push((abi::Resource::texture(planes.base.0.as_raw(), planes.base.1), abi::BUFFER_HUDLESS_COLOR, planes.size));
            resources.push((abi::Resource::texture(over.as_raw(), over_state), abi::BUFFER_UI_COLOR_AND_ALPHA, planes.size));
        } else if let Some(copy) = &hudless {
            // One plane with the interface drawn into it: the copy taken in
            // front of the interface composite (ours, always a copy's
            // destination; Streamline holds it until the tags of the frame
            // go).
            resources.push((abi::Resource::texture(copy.as_raw(), COPY_DEST), abi::BUFFER_HUDLESS_COLOR, planes.size));
            HUD_COUNTS[HUD_TAGGED].fetch_add(1, Relaxed);
        } else if hudless_wanted() {
            HUD_COUNTS[HUD_MISSING].fetch_add(1, Relaxed);
        }
        let tags: Vec<abi::ResourceTag> = resources
            .iter_mut()
            .map(|(r, kind, size)| abi::ResourceTag::new(r, *kind, abi::VALID_UNTIL_PRESENT, abi::Extent::sized(*size)))
            .collect();
        // SAFETY: tags of live textures in the states they are in at the
        // present (the line of submissions sends the present after them).
        let code = unsafe { (sl.set_tag_for_frame)(token, &viewport, tags.as_ptr(), tags.len() as u32, std::ptr::null_mut()) };
        if code != 0 {
            once!(warn, "frame generation: slSetTagForFrame: {} ({code})", abi::result_name(code));
        }
        once!(
            info,
            render = ?camera.render,
            output = ?camera.output,
            back_buffer = ?planes.size,
            interface_plane = planes.overlay.is_some(),
            interface_free_copy = hudless.is_some(),
            depth_state = format_args!("{:#x}", resources[0].0.state),
            motion_state = format_args!("{:#x}", resources[1].0.state),
            near = constants.camera_near,
            far = constants.camera_far,
            fov = constants.camera_fov,
            depth_inverted = camera.depth_inverted(),
            jitter = ?constants.jitter_offset,
            mvec_scale = ?constants.mvec_scale,
            "DLSS Frame Generation: first frame with depth and motion vectors tagged"
        );
        code == 0
    } else {
        if calls.framegen_on {
            // No upscaling this frame (a menu, a load): what was tagged may
            // no longer hold what it did.
            let tags = [abi::BUFFER_DEPTH, abi::BUFFER_MOTION_VECTORS, abi::BUFFER_HUDLESS_COLOR, abi::BUFFER_UI_COLOR_AND_ALPHA]
                .map(|kind| abi::ResourceTag::new(std::ptr::null_mut(), kind, abi::VALID_UNTIL_PRESENT, abi::Extent::default()));
            // SAFETY: null tags.
            unsafe { (sl.set_tag_for_frame)(token, &viewport, tags.as_ptr(), tags.len() as u32, std::ptr::null_mut()) };
        }
        false
    };
    // (Vilocity: the frames shown for each rendered one, changed while the
    // title runs, are trustno1-coder/VIctory-NVIDIA's, d12eb1b, GPL-3.0-or-later,
    // as this.) What the in-game menu asks (F1): off, or 2 to 4 frames shown
    // for each one rendered; 2 until it asks. The swap chain stays
    // Streamline's proxy either way.
    let wanted = framegen_wanted();
    let on = on && wanted != 0;
    let times = if on { wanted } else { 0 };
    if on != calls.framegen_on || times != calls.framegen_times {
        let mut options = abi::DlssgOptions::new(if on { abi::DLSSG_ON } else { abi::DLSSG_OFF });
        // No more than this card and driver generate (an RTX 50 makes up to
        // 3 between two rendered frames, an RTX 40 one).
        let mut generated = wanted.max(2) - 1;
        if on
            && generated > 1
            && let Some(get_state) = functions.dlssg_get_state
        {
            let mut state = abi::DlssgState::default();
            // SAFETY: a structure to fill, no options.
            let code = unsafe { get_state(&viewport, &mut state, std::ptr::null()) };
            let most = if code == 0 { state.num_frames_to_generate_max.max(1) } else { 1 };
            if generated > most {
                tracing::warn!(asked = generated + 1, given = most + 1, "DLSS Frame Generation: more frames asked than this card makes");
                generated = most;
            }
        }
        options.num_frames_to_generate = generated;
        options.flags = abi::DLSSG_RETAIN_RESOURCES_WHEN_OFF;
        options.num_back_buffers = 2;
        options.color_width = planes.size[0];
        options.color_height = planes.size[1];
        options.on_error_callback = Some(present_error);
        // SAFETY: filled options for viewport 0, on the presenting thread.
        let code = unsafe { set_options(&viewport, &options) };
        tracing::info!(frame, on, times = generated + 1, code, result = abi::result_name(code), "DLSS Frame Generation switched");
        if code == 0 {
            calls.framegen_on = on;
            // What was asked (the driver may give fewer): not set again until that changes.
            calls.framegen_times = times;
        }
    }
    if calls.framegen_on {
        FG_ON.fetch_add(1, Relaxed);
        // What it takes of the card's memory: before its first present and
        // some seconds of presents later.
        let presents = FG_ON_ALL.fetch_add(1, Relaxed) + 1;
        if (presents == 1 || presents == 240)
            && !CHECKING.load(Relaxed)
            && let Some(host) = crate::host::host()
        {
            host.log_video_memory(if presents == 1 {
                "right before the first present with frame generation on"
            } else {
                "240 presents with frame generation on later"
            });
        }
    } else {
        FG_OFF.fetch_add(1, Relaxed);
    }
    drop(calls);
    mark(abi::PRESENT_START);
    spent(T_BEFORE, began);
    Some(Presenting { token, frame })
}

// Since the last frame-rate line, microseconds: on the presenting thread
// before the present (of them in the latency markers), in the present call
// (and the longest), after it (of them asking frame generation's state); on
// the threads that record upscalings, recording one (of them waiting for our
// lock); recording the copy of the colour without the interface.
static TIMES: [AtomicU64; 9] = [const { AtomicU64::new(0) }; 9];
const T_BEFORE: usize = 0;
const T_MARKERS: usize = 1;
const T_PRESENT: usize = 2;
const T_PRESENT_LONGEST: usize = 3;
const T_AFTER: usize = 4;
const T_STATE: usize = 5;
const T_UPSCALE: usize = 6;
const T_UPSCALE_LOCK: usize = 7;
const T_COPY: usize = 8;

fn spent(slot: usize, since: std::time::Instant) {
    TIMES[slot].fetch_add(since.elapsed().as_micros() as u64, Relaxed);
}

/// When the present call of a frame [`present_begins`] began begins: `None`
/// without frame generation (nothing is timed then).
pub(crate) fn present_called(presenting: &Option<Presenting>) -> Option<std::time::Instant> {
    presenting.is_some().then(std::time::Instant::now)
}

/// The present call that began at `called` returned.
pub(crate) fn present_returned(called: Option<std::time::Instant>) {
    if let Some(called) = called {
        let took = called.elapsed().as_micros() as u64;
        TIMES[T_PRESENT].fetch_add(took, Relaxed);
        TIMES[T_PRESENT_LONGEST].fetch_max(took, Relaxed);
    }
}

unsafe extern "C" fn present_error(error: *const i32) {
    // SAFETY: the error for the call.
    let code = unsafe { error.as_ref() }.copied().unwrap_or(0);
    once!(warn, hresult = format_args!("{:#x}", code as u32), "DLSS Frame Generation: a present failed");
}

/// After our swap chain's Present: the frame's last marker, the next
/// frame's first, and what frame generation reports.
pub(crate) fn present_ends(presenting: Option<Presenting>) {
    let Some(Presenting { token, frame }) = presenting else { return };
    let (Some(sl), Some(functions)) = (SL.get(), FUNCTIONS.get()) else { return };
    let (Some(marker), Some(get_state)) = (functions.pcl_set_marker, functions.dlssg_get_state) else { return };
    let began = std::time::Instant::now();
    // SAFETY: a marker for this frame's token.
    unsafe { marker(abi::PRESENT_END, token) };
    let mut calls = calls();
    if let Some(next) = frame_token(sl, frame.wrapping_add(1)) {
        // SAFETY: as above, for the next frame.
        unsafe { marker(abi::SIMULATION_START, next) };
        calls.simulating = Some(frame.wrapping_add(1));
    }
    spent(T_MARKERS, began);
    if calls.framegen_on {
        let mut state = abi::DlssgState::default();
        let viewport = abi::ViewportHandle::new(VIEWPORT_FG);
        let asked = std::time::Instant::now();
        // SAFETY: a structure to fill, no options (no estimate asked).
        let code = unsafe { get_state(&viewport, &mut state, std::ptr::null()) };
        spent(T_STATE, asked);
        if code == 0 {
            FG_SHOWN.fetch_add(u64::from(state.num_frames_actually_presented), Relaxed);
            FG_SHOWN_ALL.fetch_add(u64::from(state.num_frames_actually_presented), Relaxed);
            let before = FG_STATUS.swap(state.status, Relaxed);
            if state.status != before {
                tracing::warn!(
                    status = format_args!("{:#x}", state.status),
                    "DLSS Frame Generation status changed (0 ok, 1 output too small, 2 Reflex not seen, 4 HDR format, 8 constants invalid, 16 back buffer index not asked)"
                );
            }
        }
    }
    spent(T_AFTER, began);
}

/// Our swap chain's buffers are about to be made again: frame generation is
/// off until the next present with an upscaling (Streamline asks so).
/// What is asked of frame generation now: 0 off, 2 to 4 frames shown for
/// each one rendered (the in-game menu's, 2 until it asks).
fn framegen_wanted() -> u32 {
    crate::live::get(crate::live::Knob::FrameGen).map_or(2, |n| if n < 2.0 { 0 } else { (n as u32).min(4) })
}

/// Whether frame generation can be switched and changed while the title
/// runs: it was asked for at the start, so the swap chain is Streamline's.
pub(crate) fn framegen_live() -> bool {
    FG_READY.load(Acquire) && SWAP_CHAIN_PROXIED.load(Acquire)
}

pub(crate) fn resizing() {
    if !FG_READY.load(Acquire) || !SWAP_CHAIN_PROXIED.load(Acquire) {
        return;
    }
    let Some(set_options) = FUNCTIONS.get().and_then(|f| f.dlssg_set_options) else { return };
    let mut calls = calls();
    if calls.framegen_on {
        let options = abi::DlssgOptions::new(abi::DLSSG_OFF);
        let viewport = abi::ViewportHandle::new(VIEWPORT_FG);
        // SAFETY: filled options.
        unsafe { set_options(&viewport, &options) };
        calls.framegen_on = false;
        calls.framegen_times = 0;
    }
}

/// `vilocity native upscaler-check`: Streamline, DLSS and DLSS Frame Generation on
/// this PC, without the title. Always: the library loads (signature,
/// `slInit`) and says what this GPU supports. With `device`: the host device
/// is made and handed over (versions, the queue through the device proxy
/// for frame generation), DLSS's render sizes for `output` are listed, and
/// DLSS runs three times on stand-in textures of the sizes `mode` gives.
/// With `present` too: three seconds of presents in a window of ours, with
/// stand-in depth and motion vectors tagged, frame generation on. The lines
/// say what happened.
pub fn check(output: [u32; 2], device: bool, present: bool) -> Vec<String> {
    let mut lines = check_run(output, device, present);
    lines.extend(shut_down());
    lines
}

/// The host device was made for [`check`].
static CHECK_DEVICE: AtomicBool = AtomicBool::new(false);

/// The end of [`check`] with a device: frame generation off, our swap chain
/// let go, Streamline shut down (`slShutdown`), before the process ends.
/// Streamline's guide asks for the shutdown before the devices go. Without
/// it the process failed fast at its exit after `--present` (exit code
/// 0xC0000409, which some shells show as 9), with frame generation on and
/// its swap chain still up; with it the exit code is 0 (2026-11-27, SDK
/// 2.14.16). Nothing of Streamline is called afterwards.
/// `VILOCITY_CHECK_TEARDOWN=0`: left out.
fn shut_down() -> Option<String> {
    if !CHECK_DEVICE.load(Relaxed) || std::env::var("VILOCITY_CHECK_TEARDOWN").is_ok_and(|v| v.trim() == "0") {
        return None;
    }
    let (sl, host) = (SL.get()?, crate::host::host()?);
    resizing();
    host.drop_presenter();
    for ready in [&FG_READY, &DLSS_READY, &SWAP_CHAIN_PROXIED, &HUDLESS_WANTED] {
        ready.store(false, Release);
    }
    *lock(&SUBMITTED) = None;
    lock(&HUD_SUBMITTED).clear();
    // SAFETY: initialised; nothing of it is in use or called from here on.
    let code = unsafe { (sl.shutdown)() };
    Some(format!("Streamline shut down: {} ({code})", abi::result_name(code)))
}

fn check_run(output: [u32; 2], device: bool, present: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut asked = match Asked::from_env(true) {
        Ok(asked) => asked,
        Err(why) => return vec![why],
    };
    // (`VILOCITY_STREAMLINE_PRESENT=1` is for DLSS without frame generation.)
    asked.framegen = !asked.present_hook;
    let available = match load_and_init(&asked, false) {
        Ok(available) => available,
        Err(why) => return vec![format!("Streamline did not load from {}: {why}", folder(&asked).display())],
    };
    lines.push(format!(
        "Streamline loaded from {}: DLSS {}, frame generation {} on this GPU",
        folder(&asked).display(),
        if available.dlss { "supported" } else { "NOT supported" },
        if available.framegen {
            "supported"
        } else if asked.framegen {
            "NOT supported"
        } else {
            "not asked for (VILOCITY_STREAMLINE_PRESENT=1)"
        }
    ));
    if !device {
        return lines;
    }
    let Some(host) = crate::host::host() else {
        lines.push("no host D3D12 device".into());
        return lines;
    };
    CHECK_DEVICE.store(true, Relaxed);
    let (dlss, framegen) = (DLSS_READY.load(Acquire), FG_READY.load(Acquire));
    lines.push(format!("device handed over: DLSS ready {dlss}, frame generation ready {framegen} (queue through Streamline's device proxy)"));
    let render = asked.mode.render_size(output);
    let camera = plan::Camera {
        jitter: [0.0, 0.0],
        motion_scale: [render[0] as f32, render[1] as f32],
        render,
        output,
        // (The far plane first, as the title names them.)
        near: 10_000.0,
        far: 0.15,
        fov_y: 1.0,
        reset: true,
        flags: plan::FFX_HIGH_DYNAMIC_RANGE | plan::FFX_DEPTH_INVERTED | plan::FFX_AUTO_EXPOSURE,
    };
    let device = &host.device;
    let made = (|| -> windows::core::Result<_> {
        use windows::Win32::Graphics::Dxgi::Common::*;
        let depth = texture(device, render, DXGI_FORMAT_D32_FLOAT, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL, SHADER_READ)?;
        let motion = texture(device, render, DXGI_FORMAT_R16G16_FLOAT, D3D12_RESOURCE_FLAG_NONE, SHADER_READ)?;
        let colour = texture(device, render, DXGI_FORMAT_R16G16B16A16_FLOAT, D3D12_RESOURCE_FLAG_NONE, SHADER_READ)?;
        let out = texture(device, output, DXGI_FORMAT_R16G16B16A16_FLOAT, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, UNORDERED)?;
        let base = texture(device, output, DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_NONE, 0)?;
        Ok((depth, motion, colour, out, base))
    })();
    let (depth, motion, colour, out, base) = match made {
        Ok(made) => made,
        Err(e) => {
            lines.push(format!("stand-in textures not made: {e}"));
            return lines;
        }
    };
    if dlss {
        // (Known once the first evaluation asked for them.)
        let sizes = || -> String {
            let calls = calls();
            let sizes: Vec<String> = calls
                .optimal
                .iter()
                .filter(|(o, _, _)| *o == output)
                .map(|&(_, m, s)| format!("{} {:?}", plan::sl_mode_name(m), s))
                .collect();
            if sizes.is_empty() { "?".into() } else { sizes.join(", ") }
        };
        let mut textures: Textures = Default::default();
        textures[COLOUR] = Some((colour, SHADER_READ));
        textures[DEPTH] = Some((depth.clone(), SHADER_READ));
        textures[MOTION] = Some((motion.clone(), SHADER_READ));
        textures[OUTPUT] = Some((out, UNORDERED));
        // (`VILOCITY_STREAMLINE_PRESENT=1`: DLSS alone, with a present through
        // Streamline's proxy of our swap chain after each upscaling, more of
        // them than Streamline waits for a present.)
        let hooked = present && asked.present_hook && !framegen;
        for round in 0..if hooked { 120 } else { 3 } {
            if hooked && round > 0 {
                host.present_planes(&base, 0, None, round);
            }
            let id = NEXT_ID.fetch_add(1, Relaxed) + 1;
            let list = match host.new_list() {
                Ok(list) => list,
                Err(e) => {
                    lines.push(format!("no command list: {e}"));
                    return lines;
                }
            };
            let recorded = record(list.list.as_raw(), &textures, &camera, 1.0, id);
            list.close();
            host.execute_sync_internal(&list);
            // SAFETY: a plain query.
            let removed = unsafe { device.GetDeviceRemovedReason() };
            match (recorded, removed) {
                (Ok(r), Ok(())) if round == 0 => lines.push(format!(
                    "DLSS ran on the GPU: {render:?} -> {output:?} as {} (optimal {:?}); render sizes for {output:?}: {}",
                    plan::sl_mode_name(r.mode),
                    r.optimal,
                    sizes()
                )),
                (Ok(_), Ok(())) => {}
                (Err(why), _) => {
                    lines.push(format!("DLSS did not run: {why}"));
                    return lines;
                }
                (_, Err(e)) => {
                    lines.push(format!("the device was removed after DLSS ran: {e}"));
                    return lines;
                }
            }
        }
        lines.push(if hooked {
            format!(
                "DLSS ran 120 times on stand-in textures with a present after each; our swap chain is {}Streamline's proxy of it (VILOCITY_STREAMLINE_PRESENT=1); the device is fine",
                if PRESENT_HOOKED.load(Relaxed) { "" } else { "NOT " }
            )
        } else {
            "DLSS ran three times on stand-in textures; the device is fine".into()
        });
    }
    if present && framegen {
        CHECKING.store(true, Relaxed);
        let (on, shown) = (FG_ON_ALL.load(Relaxed), FG_SHOWN_ALL.load(Relaxed));
        // A stand-in for the colour without the interface, tagged as the
        // copies in front of the title's interface composite are: ours, a
        // copy's destination, of the back buffer's size and format.
        let copy = {
            use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM;
            texture(device, output, DXGI_FORMAT_R8G8B8A8_UNORM, D3D12_RESOURCE_FLAG_NONE, COPY_DEST).ok()
        };
        let tagged = HUD_COUNTS[HUD_TAGGED].load(Relaxed);
        for frame in 1..=180u64 {
            let id = NEXT_ID.fetch_add(1, Relaxed) + 1;
            *SUBMITTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Noted {
                id,
                depth: Source::Fixed(depth.clone(), SHADER_READ),
                motion: Source::Fixed(motion.clone(), SHADER_READ),
                camera,
            });
            if let Some(copy) = &copy {
                lock(&HUD_SUBMITTED).push(Copied { id: frame, target: base.as_raw() as usize, texture: copy.clone() });
            }
            host.present_planes(&base, 0, None, frame);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        crate::host::settle_for_fence();
        lines.push(format!(
            "frame generation: 180 presents, {} with it on, {} frames shown by it, {} presents tagged with a colour without the interface, status {:#x} (0 is fine){}",
            FG_ON_ALL.load(Relaxed) - on,
            FG_SHOWN_ALL.load(Relaxed) - shown,
            HUD_COUNTS[HUD_TAGGED].load(Relaxed) - tagged,
            FG_STATUS.load(Relaxed),
            if SWAP_CHAIN_PROXIED.load(Relaxed) { "" } else { "; the swap chain is NOT Streamline's proxy" }
        ));
        // SAFETY: a plain query.
        if let Err(e) = unsafe { device.GetDeviceRemovedReason() } {
            lines.push(format!("the device was removed while presenting: {e}"));
        }
    } else if present {
        lines.push("frame generation is not ready: no presents tried".into());
    }
    lines
}

const SHADER_READ: u32 = 0xc0;
const UNORDERED: u32 = 0x8;
const COPY_DEST: u32 = 0x400;

/// A stand-in texture for [`check`] (its contents are whatever the memory held).
fn texture(
    device: &ID3D12Device,
    size: [u32; 2],
    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    flags: D3D12_RESOURCE_FLAGS,
    state: u32,
) -> windows::core::Result<ID3D12Resource> {
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: 0,
        Width: u64::from(size[0]),
        Height: size[1],
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: format,
        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        Flags: flags,
    };
    let heap = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() };
    let mut made: Option<ID3D12Resource> = None;
    // SAFETY: plain texture creation.
    unsafe {
        device.CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATES(state as i32), None, &mut made)?
    };
    made.ok_or_else(windows::core::Error::empty)
}

/// For the frame-rate line.
pub(crate) fn line(frames: f64) {
    let Some(plan) = PLAN.get() else { return };
    let [done, failed, forwarded, on, off, shown] =
        [&EVALUATED, &FAILED, &FORWARDED, &FG_ON, &FG_OFF, &FG_SHOWN].map(|c| c.swap(0, Relaxed));
    if plan.dlss || forwarded > 0 {
        tracing::info!(
            "DLSS per frame: {:.2} upscalings by DLSS, {:.2} failed, {:.2} left to the title's FidelityFX ({} by DLSS so far)",
            done as f64 / frames,
            failed as f64 / frames,
            forwarded as f64 / frames,
            EVALUATED_ALL.load(Relaxed)
        );
    }
    if plan.framegen {
        tracing::info!(
            "DLSS Frame Generation: {on} presents with it on, {off} off, {shown} frames shown by it (status {:#x}){}",
            FG_STATUS.load(Relaxed),
            if FG_READY.load(Relaxed) && SWAP_CHAIN_PROXIED.load(Relaxed) { "" } else { "; not running (see above)" }
        );
        // Where the time of a frame goes in Streamline's calls.
        let t: Vec<u64> = TIMES.iter().map(|t| t.swap(0, Relaxed)).collect();
        if on + off > 0 {
            let ms = |slot: usize| t[slot] as f64 / 1000.0 / frames;
            tracing::info!(
                "DLSS Frame Generation, time per frame: the presenting thread {:.2} ms before the present call, {:.2} ms in it (the longest {:.1} ms), {:.2} ms after it; of those {:.2} ms in the latency markers and {:.2} ms asking frame generation's state; an upscaling recorded in {:.2} ms ({:.2} ms of it waiting for the lock it shares with the present); the copy of the colour without the interface recorded in {:.3} ms",
                ms(T_BEFORE),
                ms(T_PRESENT),
                t[T_PRESENT_LONGEST] as f64 / 1000.0,
                ms(T_AFTER),
                ms(T_MARKERS),
                ms(T_STATE),
                ms(T_UPSCALE),
                ms(T_UPSCALE_LOCK),
                ms(T_COPY)
            );
        }
        // Every tenth line (about ten seconds): the colour without the
        // interface, where the title draws the interface into the frame.
        static LINES: AtomicU32 = AtomicU32::new(0);
        if hudless_wanted() && LINES.fetch_add(1, Relaxed) % 10 == 9 {
            let [copies, tagged, missing, composites, again, others] = [0, 1, 2, 3, 4, 5].map(|i| HUD_COUNTS[i].swap(0, Relaxed));
            tracing::info!(
                "DLSS Frame Generation, colour without the interface, last ten lines: {copies} copies in front of the title's interface composite, {tagged} presents tagged with one, {missing} presents with frame generation on had none; {composites} interface composite draws into a presented texture ({again} after a copy in their list), {others} draws of other pipelines into one"
            );
        }
    }
}

// (Vilocity) What must hold of a run without the switch, or without the
// library: nothing is asked for, loaded, served or recorded.
#[cfg(test)]
mod tests {
    use super::*;

    fn vars<'a>(set: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| set.iter().find(|(n, _)| *n == name).map(|(_, v)| (*v).to_owned())
    }

    /// Nothing of Streamline is there, and what the hooks call does nothing.
    fn nothing_runs() {
        assert!(SL.get().is_none() && PLAN.get().is_none() && FUNCTIONS.get().is_none() && PROXIES.get().is_none());
        assert!(!serving() && !upscaling() && !hudless_wanted());
        let up = Upscale {
            context: 0,
            list: std::ptr::null(),
            resources: [(std::ptr::null(), 0); 7],
            camera: plan::Camera {
                jitter: [0.0; 2],
                motion_scale: [1.0; 2],
                render: [1280, 720],
                output: [1920, 1080],
                near: 0.1,
                far: 100.0,
                fov_y: 1.0,
                reset: false,
                flags: 0,
            },
            pre_exposure: 1.0,
            frame_ms: 16.0,
        };
        assert_eq!(upscale(&up), Taken::No("Streamline is not loaded"));
        // The hooks of the host: submissions, the present, a resize, the
        // frame-rate line, a context destroyed.
        submitted(1);
        hudless_submitted(1);
        assert!(present_called(&None).is_none());
        present_returned(None);
        present_ends(None);
        resizing();
        line(1.0);
        upscaler_destroyed(1);
        assert!(lock(&PENDING).is_empty() && lock(&SUBMITTED).is_none() && lock(&HUD_SUBMITTED).is_empty());
        assert_eq!(EVALUATED_ALL.load(Relaxed) + FORWARDED.load(Relaxed) + FG_ON_ALL.load(Relaxed), 0);
        assert!(!DLSS_READY.load(Relaxed) && !FG_READY.load(Relaxed) && !SWAP_CHAIN_PROXIED.load(Relaxed));
    }

    #[test]
    fn without_the_switch_nothing_is_asked_loaded_or_served() {
        // Every other variable set, two of them to what would be an error.
        let others = [
            ("VILOCITY_FRAMEGEN", "1"),
            ("VILOCITY_DLSS_MODE", "no such mode"),
            ("VILOCITY_DLSS_PRESET", "no such preset"),
            ("VILOCITY_STREAMLINE_DIR", r"C:\Windows\System32"),
            ("VILOCITY_STREAMLINE_PRESENT", "1"),
        ];
        for dlss in [None, Some("0"), Some(""), Some("true"), Some("11")] {
            let mut set = others.to_vec();
            set.extend(dlss.map(|v| ("VILOCITY_DLSS", v)));
            let asked = Asked::read(vars(&set), false).unwrap();
            assert!(!asked.dlss && !asked.framegen && !asked.any(), "{dlss:?}");
            assert_eq!(init(&asked), Available::default());
        }
        nothing_runs();
    }

    #[test]
    fn with_the_switch_the_library_must_be_in_the_folder_named() {
        // No folder named, a folder that is not there, a folder without the
        // library: nothing is loaded.
        let missing = std::env::temp_dir().join("vilocity-no-streamline-here");
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        for dir in [None, Some(missing.to_string_lossy().into_owned()), Some(system), Some("  ".into())] {
            let mut set = vec![("VILOCITY_DLSS", "1"), ("VILOCITY_FRAMEGEN", "1")];
            set.extend(dir.as_deref().map(|d| ("VILOCITY_STREAMLINE_DIR", d)));
            let asked = Asked::read(vars(&set), false).unwrap();
            assert!(asked.dlss && asked.framegen && asked.any());
            assert_eq!(asked.dir.is_some(), dir.as_deref().is_some_and(|d| !d.trim().is_empty()));
            assert_eq!(init(&asked), Available::default(), "{dir:?}");
        }
        nothing_runs();
    }

    /// The signature check finds `WinVerifyTrust` when it is asked and
    /// refuses a file without a signature (this test program): 0x800b0100,
    /// no signature.
    #[test]
    fn a_library_without_a_signature_is_refused() {
        let refused = verify_signature(&std::env::current_exe().unwrap()).unwrap_err();
        assert!(refused.ends_with("is not validly signed (0x800b0100)"), "{refused}");
    }

    #[test]
    fn the_switches_read_as_they_are_named() {
        let asked = Asked::read(vars(&[("VILOCITY_DLSS", " 1 ")]), false).unwrap();
        assert!(asked.dlss && !asked.framegen && asked.dir.is_none());
        assert_eq!((asked.mode, asked.preset), (Mode::Quality, 0));
        assert_eq!((asked.jitter, asked.mvec, asked.reactive, asked.hudless), ([1.0; 2], [1.0; 2], true, true));
        let asked = Asked::read(
            vars(&[
                ("VILOCITY_DLSS", "1"),
                ("VILOCITY_FRAMEGEN", "1"),
                ("VILOCITY_DLSS_MODE", "Performance"),
                ("VILOCITY_DLSS_PRESET", "k"),
                ("VILOCITY_STREAMLINE_DIR", r"D:\sdk\bin\x64"),
                ("VILOCITY_DLSS_JITTER", "-1,1"),
                ("VILOCITY_DLSS_REACTIVE", "0"),
            ]),
            false,
        )
        .unwrap();
        assert!(asked.framegen && !asked.reactive);
        assert_eq!((asked.mode, asked.preset, asked.jitter), (Mode::Performance, 11, [-1.0, 1.0]));
        assert_eq!(asked.dir.as_deref(), Some(Path::new(r"D:\sdk\bin\x64")));
        // With the switch, a name that is none is an error.
        assert!(Asked::read(vars(&[("VILOCITY_DLSS", "1"), ("VILOCITY_DLSS_MODE", "fast")]), false).is_err());
        assert!(Asked::read(vars(&[("VILOCITY_DLSS", "1"), ("VILOCITY_DLSS_PRESET", "z")]), false).is_err());
        // The check without the title asks as with the switch.
        assert!(Asked::read(vars(&[]), true).unwrap().dlss);
        assert!(Asked::read(vars(&[("VILOCITY_DLSS_MODE", "fast")]), true).is_err());
    }
}
