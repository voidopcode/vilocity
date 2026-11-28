//! (Vilocity) AMD FSR frame generation (`VILOCITY_FSR_FRAMEGEN=1`): a made-up picture between
//! every two the game renders, by AMD's FidelityFX SDK (v2.4.0, MIT; `scripts\get-fsr.ps1`).
//! The game never asks for it (its console build has only the upscaler), so it is put in the
//! way AMD's integration guide puts it: our swap chain is wrapped by the SDK's frame
//! generation swap chain (`ffxCreateContextDescFrameGenerationSwapChainForHwndDX12`), and right
//! after each of the game's FSR upscales (the PC upscaler's, `vilocity-hle`'s `fidelityfx`)
//! the frame's depth, motion vectors and camera go to the frame generation "prepare" pass on
//! the same command list, with the frame configured just before. The DLL is
//! `amd_fidelityfx_framegeneration_dx12.dll` in the folder of `VILOCITY_FSR_DLL` (the
//! sandbox may read it), signed by AMD. Without it, or with the switch off, nothing changes.
//! shortcut: no HUD-less picture is given, so the game's HUD is interpolated with the scene
//! (the console composes it on a plane of its own: that plane could be given later).

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Graphics::Direct3D12::ID3D12CommandQueue;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dxgi::{DXGI_SWAP_CHAIN_DESC1, IDXGIFactory2, IDXGISwapChain3, IDXGISwapChain4};
use windows::core::Interface;

/// The SDK's description types (`ffx_api.h`, `ffx_framegeneration.h`, `ffx_api_dx12.h`,
/// `ffx_api_framegeneration_dx12.h`).
const CREATE_BACKEND_DX12: u64 = 0x0000_0002;
const CREATE_FRAMEGENERATION: u64 = 0x0002_0001;
const CONFIGURE_FRAMEGENERATION: u64 = 0x0002_0002;
const DISPATCH_FRAMEGENERATION_PREPARE: u64 = 0x0002_0004;
const CREATE_SWAPCHAIN_FOR_HWND_DX12: u64 = 0x0003_0006;
const QUERY_GET_VERSIONS: u64 = 4;
const OVERRIDE_VERSION: u64 = 5;
/// A version id with this bit is the AMD driver's own provider (`ffx_provider.h`).
const DRIVER_PROVIDER: u64 = 1 << 31;
const OK: u32 = 0;

/// `FfxApiCreateContextFramegenerationFlags`.
const FG_DEPTH_INVERTED: u32 = 1 << 3;
const FG_DEPTH_INFINITE: u32 = 1 << 4;
const FG_JITTERED_MOTION: u32 = 1 << 2;
const FG_DISPLAY_SIZE_MOTION: u32 = 1 << 1;
/// The upscaler's own flags of the same meaning (`FfxApiCreateContextUpscaleFlags`).
const UP_DISPLAY_SIZE_MOTION: u32 = 1 << 1;
const UP_JITTERED_MOTION: u32 = 1 << 2;
const UP_DEPTH_INVERTED: u32 = 1 << 3;
const UP_DEPTH_INFINITE: u32 = 1 << 4;

/// `ffxApiHeader`: the type, then the next description in the chain.
#[repr(C)]
struct Header {
    kind: u64,
    next: *mut Header,
}

impl Header {
    const fn of(kind: u64) -> Header {
        Header { kind, next: null_mut() }
    }
}

/// `FfxApiResource`: the resource, its description (type, format, width, height, depth,
/// levels, flags, usage) and its state: 48 bytes aligned as the pointer it starts with, as
/// the game's upscale description holds them (`vilocity-hle`'s `fidelityfx` copies them
/// out after making them the host's).
#[derive(Clone, Copy)]
#[repr(C, align(8))]
pub struct Resource(pub [u8; 48]);

#[repr(C)]
struct BackendDx12 {
    header: Header,
    device: *mut c_void,
}

#[repr(C)]
struct CreateFrameGeneration {
    header: Header,
    flags: u32,
    display_size: [u32; 2],
    max_render_size: [u32; 2],
    back_buffer_format: u32,
}

#[repr(C)]
struct SwapChainForHwnd {
    header: Header,
    swap_chain: *mut *mut c_void,
    hwnd: *mut c_void,
    desc: *const c_void,
    fullscreen_desc: *const c_void,
    factory: *mut c_void,
    game_queue: *mut c_void,
}

type DispatchCallback = unsafe extern "C" fn(params: *mut Header, user: *mut c_void) -> u32;

#[repr(C)]
struct GetVersions {
    header: Header,
    create_type: u64,
    device: *mut c_void,
    count: *mut u64,
    ids: *mut u64,
    names: *mut *const std::ffi::c_char,
}

#[repr(C)]
struct OverrideVersion {
    header: Header,
    id: u64,
}

#[repr(C)]
struct ConfigureFrameGeneration {
    header: Header,
    swap_chain: *mut c_void,
    present_callback: *const c_void,
    present_user: *mut c_void,
    generation_callback: Option<DispatchCallback>,
    generation_user: *mut c_void,
    enabled: bool,
    allow_async: bool,
    hudless: Resource,
    flags: u32,
    only_generated: bool,
    rect: [i32; 4],
    frame: u64,
}

#[repr(C)]
struct Prepare {
    header: Header,
    frame: u64,
    flags: u32,
    command_list: *mut c_void,
    render_size: [u32; 2],
    jitter: [f32; 2],
    motion_scale: [f32; 2],
    frame_ms: f32,
    unused_reset: bool,
    near: f32,
    far: f32,
    fov_y: f32,
    meters: f32,
    depth: Resource,
    motion: Resource,
}

type Create = unsafe extern "C" fn(context: *mut *mut c_void, desc: *mut Header, memory: *const c_void) -> u32;
type Call = unsafe extern "C" fn(context: *mut *mut c_void, desc: *mut Header) -> u32;

struct Api {
    create: Create,
    configure: Call,
    dispatch: Call,
    query: Call,
}

/// What a frame of the game gives frame generation: the upscale's sizes, camera and inputs.
pub struct Frame {
    pub render: [u32; 2],
    pub jitter: [f32; 2],
    pub motion_scale: [f32; 2],
    pub frame_ms: f32,
    pub near: f32,
    pub far: f32,
    pub fov_y: f32,
    pub meters: f32,
    /// The upscaler context's creation flags (depth inverted and so on), and its largest
    /// render size (zero if not known: this frame's is taken).
    pub upscale_flags: u32,
    pub max_render: [u32; 2],
    pub depth: Resource,
    pub motion: Resource,
}

struct State {
    /// The wrapped swap chain (the SDK's), and its size and FidelityFX surface format.
    swap_chain: usize,
    display: [u32; 2],
    format: u32,
    /// The frame generation context, made at the first frame (`None`: not yet; it is
    /// boxed so that the dispatch callback can be handed a pointer that stays).
    context: Option<Box<*mut c_void>>,
    frame: u64,
    failed: bool,
}

// SAFETY: the raw pointers are the SDK's objects, used under the mutex.
unsafe impl Send for State {}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// `VILOCITY_FSR_FRAMEGEN=1` (the play script sets it from the menu's frame generation on
/// cards other than NVIDIA's, with AMD's DLLs there).
pub fn wanted() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VILOCITY_FSR_FRAMEGEN").is_some_and(|v| v == "1"))
}

fn path() -> Option<PathBuf> {
    let upscaler = PathBuf::from(std::env::var_os("VILOCITY_FSR_DLL")?);
    Some(upscaler.with_file_name("amd_fidelityfx_framegeneration_dx12.dll"))
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        let path = path()?;
        let loaded = load(&path);
        match &loaded {
            Ok(_) => tracing::info!(path = %path.display(), "FSR frame generation: AMD's DLL loaded"),
            Err(e) => tracing::warn!(path = %path.display(), error = %e, "FSR frame generation: AMD's DLL cannot be used; no frame generation"),
        }
        loaded.ok()
    })
    .as_ref()
}

fn load(path: &std::path::Path) -> Result<Api, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW};
    if !path.is_file() {
        return Err("not there (scripts\\get-fsr.ps1 gets it)".into());
    }
    crate::streamline::verify_signature(path)?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a NUL-terminated path; the DLL's folder is searched for what it imports.
    let module = unsafe { LoadLibraryExW(wide.as_ptr(), null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) };
    if module.is_null() {
        return Err(format!("LoadLibrary failed: {}", std::io::Error::last_os_error()));
    }
    let export = |name: &'static [u8]| {
        // SAFETY: a loaded module and a NUL-terminated name.
        unsafe { GetProcAddress(module, name.as_ptr()) }
            .map(|f| f as usize)
            .ok_or_else(|| format!("no export {}", String::from_utf8_lossy(&name[..name.len() - 1])))
    };
    // SAFETY: every FidelityFX API DLL exports these with these signatures (`ffx_api.h`).
    unsafe {
        Ok(Api {
            create: std::mem::transmute::<usize, Create>(export(b"ffxCreateContext\0")?),
            configure: std::mem::transmute::<usize, Call>(export(b"ffxConfigure\0")?),
            dispatch: std::mem::transmute::<usize, Call>(export(b"ffxDispatch\0")?),
            query: std::mem::transmute::<usize, Call>(export(b"ffxQuery\0")?),
        })
    }
}

/// The version of AMD's DLL to pin for an effect (`create_type`): its own provider (not the
/// AMD driver's, which the SDK takes when newer) whose name `fits`. So every card runs the
/// one provider this was tested with, on NVIDIA too: unpinned, an AMD card may get the
/// driver's, or the DLL's ML frame generation, which needs camera vectors the game's
/// upscale does not give (`ffx_provider.h`, `ffx_framegeneration.h`, SDK v2.4.0).
/// `None` (no pin: the SDK chooses) if the DLL lists none that fits.
fn own_version(api: &Api, create_type: u64, device: *mut c_void, fits: fn(&str) -> bool) -> Option<u64> {
    const MOST: usize = 16;
    let mut count = MOST as u64;
    let mut ids = [0u64; MOST];
    let mut names = [null::<std::ffi::c_char>(); MOST];
    let mut query = GetVersions {
        header: Header::of(QUERY_GET_VERSIONS),
        create_type,
        device,
        count: &mut count,
        ids: ids.as_mut_ptr(),
        names: names.as_mut_ptr(),
    };
    // SAFETY: the SDK's query of versions, without a context, into arrays of `count`.
    let code = unsafe { (api.query)(null_mut(), &mut query.header) };
    if code != OK {
        tracing::warn!(code, create_type = format_args!("{create_type:#x}"), "FSR frame generation: AMD's DLL lists no versions; its own choice is taken");
        return None;
    }
    let listed: Vec<(u64, String)> = (0..(count as usize).min(MOST))
        .map(|i| {
            // SAFETY: the SDK's NUL-terminated names, static in its DLL.
            let name = (!names[i].is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(names[i]) }.to_string_lossy().into_owned());
            (ids[i], name.unwrap_or_default())
        })
        .collect();
    let pinned = listed.iter().find(|(id, name)| id & DRIVER_PROVIDER == 0 && fits(name)).map(|(id, _)| *id);
    tracing::info!(create_type = format_args!("{create_type:#x}"), ?listed, pinned = ?pinned.map(|id| format!("{id:#x}")), "FSR frame generation: AMD's versions");
    pinned
}

/// The FidelityFX surface format of a swap chain's DXGI format (`FfxApiSurfaceFormat`).
fn surface_format(dxgi: u32) -> Option<u32> {
    match dxgi {
        28 => Some(10), // R8G8B8A8_UNORM
        29 => Some(11), // R8G8B8A8_UNORM_SRGB
        87 => Some(14), // B8G8R8A8_UNORM
        24 => Some(17), // R10G10B10A2_UNORM
        10 => Some(4),  // R16G16B16A16_FLOAT
        _ => None,
    }
}

/// The SDK's frame generation swap chain for `hwnd`, made by the SDK from `desc`, when it is
/// wanted and AMD's DLL is there. `None` otherwise or when the SDK refuses: nothing was made
/// for the window then, and the caller makes its own as before. (The SDK's wrap of a swap
/// chain we made releases ours before making its own, so a refusal there left none: one
/// flip-model swap chain per window, so it refused while we held ours, 2026-11-28.)
pub(crate) fn for_window(
    factory: &IDXGIFactory2,
    hwnd: HWND,
    desc: &DXGI_SWAP_CHAIN_DESC1,
    queue: &ID3D12CommandQueue,
) -> Option<IDXGISwapChain3> {
    if !wanted() {
        return None;
    }
    let dxgi_format = desc.Format.0 as u32;
    let (Some(api), Some(format)) = (api(), surface_format(dxgi_format)) else {
        tracing::warn!(dxgi_format, "FSR frame generation: off (no DLL, or a swap chain format it does not take)");
        return None;
    };
    let mut raw: *mut c_void = null_mut();
    let mut make = SwapChainForHwnd {
        header: Header::of(CREATE_SWAPCHAIN_FOR_HWND_DX12),
        swap_chain: &mut raw,
        hwnd: hwnd.0,
        desc: (desc as *const DXGI_SWAP_CHAIN_DESC1).cast(),
        fullscreen_desc: null(),
        factory: factory.as_raw(),
        game_queue: queue.as_raw(),
    };
    let mut pin = OverrideVersion { header: Header::of(OVERRIDE_VERSION), id: 0 };
    if let Some(id) = own_version(api, CREATE_SWAPCHAIN_FOR_HWND_DX12, null_mut(), |_| true) {
        pin.id = id;
        make.header.next = &mut pin.header;
    }
    let mut context: *mut c_void = null_mut();
    // SAFETY: the SDK's create with its descriptions, all of whose pointers outlive the call.
    let code = unsafe { (api.create)(&mut context, &mut make.header, null()) };
    if code != OK || raw.is_null() {
        tracing::warn!(code, "FSR frame generation: the SDK made no swap chain; no frame generation");
        return None;
    }
    // SAFETY: the SDK's new swap chain, its one reference ours.
    let made = unsafe { IDXGISwapChain4::from_raw(raw) };
    let size = [desc.Width, desc.Height];
    tracing::info!(width = size[0], height = size[1], format, "FSR frame generation: the swap chain is the SDK's");
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(State { swap_chain: made.as_raw() as usize, display: size, format, context: None, frame: 0, failed: false });
    made.cast().ok()
}

/// The SDK asks for its interpolation work here, once a frame, with our context.
unsafe extern "C" fn generate(params: *mut Header, user: *mut c_void) -> u32 {
    let Some(api) = api() else { return 1 };
    // SAFETY: `user` is the boxed context handed to configure; `params` the SDK's own.
    let code = unsafe { (api.dispatch)(user.cast(), params) };
    // The made-up frames, counted in the log (the game's own frame rate counts only its own).
    static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if code == OK {
        let made = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if made == 1 || made % 1000 == 0 {
            tracing::info!(made, "FSR frame generation: made-up frames so far");
        }
    } else {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| tracing::warn!(code, "FSR frame generation: a made-up frame was refused (said once)"));
    }
    code
}

/// One frame of the game, right after its upscale was recorded on `list` (the host's
/// command list): the frame is configured and its prepare pass recorded on the same list.
/// Does nothing unless the swap chain was wrapped.
pub fn prepare(list: *mut c_void, f: &Frame) {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = state.as_mut().filter(|s| !s.failed) else { return };
    let Some(api) = api() else { return };
    if s.context.is_none() {
        let Some(host) = crate::host::host() else { return };
        let mut backend = BackendDx12 { header: Header::of(CREATE_BACKEND_DX12), device: host.device.as_raw() };
        // AMD's FSR3 frame generation ("3.x"), the open one, on every card.
        let mut pin = OverrideVersion { header: Header::of(OVERRIDE_VERSION), id: 0 };
        if let Some(id) = own_version(api, CREATE_FRAMEGENERATION, backend.device, |name| name.starts_with("3.")) {
            pin.id = id;
            backend.header.next = &mut pin.header;
        }
        let u = f.upscale_flags;
        let flags = [(UP_DEPTH_INVERTED, FG_DEPTH_INVERTED), (UP_DEPTH_INFINITE, FG_DEPTH_INFINITE), (UP_JITTERED_MOTION, FG_JITTERED_MOTION), (UP_DISPLAY_SIZE_MOTION, FG_DISPLAY_SIZE_MOTION)]
            .iter()
            .filter(|(up, _)| u & up != 0)
            .fold(0, |all, (_, fg)| all | fg);
        let mut desc = CreateFrameGeneration {
            header: Header { kind: CREATE_FRAMEGENERATION, next: &mut backend.header },
            flags,
            display_size: s.display,
            max_render_size: if f.max_render[0] > 0 { f.max_render } else { f.render },
            back_buffer_format: s.format,
        };
        let mut context: *mut c_void = null_mut();
        // SAFETY: the SDK's create with its descriptions, chained.
        let code = unsafe { (api.create)(&mut context, &mut desc.header, null()) };
        if code != OK {
            tracing::warn!(code, flags, render = ?f.render, display = ?s.display, "FSR frame generation: its context could not be made; no frame generation");
            s.failed = true;
            return;
        }
        tracing::info!(flags, render = ?f.render, display = ?s.display, "FSR frame generation: on");
        s.context = Some(Box::new(context));
    }
    let context: *mut *mut c_void = &mut **s.context.as_mut().expect("made above");
    let mut configure = ConfigureFrameGeneration {
        header: Header::of(CONFIGURE_FRAMEGENERATION),
        swap_chain: s.swap_chain as *mut c_void,
        present_callback: null(),
        present_user: null_mut(),
        generation_callback: Some(generate),
        generation_user: context.cast(),
        enabled: true,
        allow_async: false,
        hudless: Resource([0; 48]),
        flags: 0,
        only_generated: false,
        rect: [0, 0, s.display[0] as i32, s.display[1] as i32],
        frame: s.frame,
    };
    let mut prepare = Prepare {
        header: Header::of(DISPATCH_FRAMEGENERATION_PREPARE),
        frame: s.frame,
        flags: 0,
        command_list: list,
        render_size: f.render,
        jitter: f.jitter,
        motion_scale: f.motion_scale,
        frame_ms: f.frame_ms,
        unused_reset: false,
        near: f.near,
        far: f.far,
        fov_y: f.fov_y,
        meters: f.meters,
        depth: f.depth,
        motion: f.motion,
    };
    // SAFETY: the SDK's configure and dispatch on its context, with its descriptions; the
    // list is open for recording (the game's upscale was just recorded on it).
    let codes = unsafe { [(api.configure)(context, &mut configure.header), (api.dispatch)(context, &mut prepare.header)] };
    s.frame += 1;
    if codes != [OK, OK] {
        static SAID: std::sync::Once = std::sync::Once::new();
        SAID.call_once(|| tracing::warn!(?codes, "FSR frame generation: a frame was refused (said once)"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The descriptions laid out as the SDK's headers have them.
    #[test]
    fn descriptions_lay_out_as_the_sdk() {
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<CreateFrameGeneration>(), 40);
        assert_eq!(offset_of!(ConfigureFrameGeneration, enabled), 56);
        assert_eq!(offset_of!(ConfigureFrameGeneration, hudless), 64);
        assert_eq!(offset_of!(ConfigureFrameGeneration, flags), 112);
        assert_eq!(offset_of!(ConfigureFrameGeneration, rect), 120);
        assert_eq!(offset_of!(ConfigureFrameGeneration, frame), 136);
        assert_eq!(offset_of!(Prepare, command_list), 32);
        assert_eq!(offset_of!(Prepare, frame_ms), 64);
        assert_eq!(offset_of!(Prepare, near), 72);
        assert_eq!(offset_of!(Prepare, depth), 88);
        assert_eq!(size_of::<Prepare>(), 184);
        assert_eq!(size_of::<SwapChainForHwnd>(), 64);
        assert_eq!(offset_of!(GetVersions, names), 48);
        assert_eq!(size_of::<GetVersions>(), 56);
        assert_eq!(size_of::<OverrideVersion>(), 24);
    }
}
