//! `amd_fidelityfx_loader_xbox.dll`, redirected to a PC FidelityFX DLL
//! (`VILOCITY_FSR_DLL`; off by default). (Neptune-VE, fitted to this tree.)
//!
//! The title upscales through AMD's FidelityFX API (`ffxCreateContext`,
//! `ffxDestroyContext`, `ffxConfigure`, `ffxQuery`, `ffxDispatch`): the
//! loader and the upscaler in its folder are console builds, mapped as
//! title code, whose shaders go through our translator. A PC FidelityFX DLL
//! exports the same five functions. With `VILOCITY_FSR_DLL=<path>`
//! (`amd_fidelityfx_upscaler_dx12.dll`, or `amd_fidelityfx_loader_dx12.dll`
//! with the upscaler beside it) the title's loader is not mapped: its
//! `LoadLibrary` gets this module, and each call goes to the PC DLL, so
//! tools that hook that DLL (OptiScaler) see the title's upscaler like any
//! PC game's. A DLL that cannot be loaded leaves the title its own, as
//! without the variable.
//!
//! The PC DLL needs real D3D12 objects where the title passes console ones:
//! the host device for the context, and in a dispatch the host command list
//! the title's list records onto and the host textures behind the title's
//! (`vilocity_gpu::external`). Descriptions are patched in place for the
//! call and put back after it. What only the console build knows is left
//! out of what the PC DLL sees: the console's backend descriptions, a
//! version override (it would name a console provider), the title's own
//! callbacks (console code, which the PC DLL could call from threads of its
//! own) and its allocation callbacks.
//!
//! Where the numbers come from: "header" ones are in the declarations of
//! the FidelityFX SDK 2.4.0 headers (`ffx_api.h`, `ffx_api_types.h`,
//! `ffx_api_dx12.h`, `ffx_upscale.h`) that Leonida (ItsNotRexed/Leonida,
//! `runtime/test/ffx_api_min.h`) writes out; "measured" ones Leonida's notes
//! measured on the title's own DLL (`docs/d3d12_vulkan.md`, appendix A.3:
//! its provider is FSR 4 INT8 "4.1.1", its create wants an upscaler version
//! description and a backend description of type `0x0100_0002` naming the
//! device); "read" ones were read from the code of the PC upscaler 4.1.0.0
//! (`amd_fidelityfx_upscaler_dx12.dll`: the table its backend turns states
//! with, and the types its create, configure, query and dispatch compare);
//! "recalled" ones are the SDK's as remembered, not checked. Neptune-VE's
//! numbers for the resource states, the version override and the reactive
//! mask dispatch were not those the PC DLL reads.

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use vilocity_gpu::external;

use crate::HleModule;

/// The environment variable that turns the redirect on.
const VARIABLE: &str = "VILOCITY_FSR_DLL";

/// The title DLLs the redirect answers for.
const DLLS: [&str; 3] = [
    "amd_fidelityfx_loader_xbox.dll",
    "amd_fidelityfx_upscaler_xbox.dll",
    "amd_fidelityfx_framegeneration_xbox.dll",
];

/// The PC loader, and the upscaler it loads by name.
const PC_LOADER: &str = "amd_fidelityfx_loader_dx12.dll";
const PC_UPSCALER: &str = "amd_fidelityfx_upscaler_dx12.dll";

// The descriptions below are C structures of 64-bit pointers.
const _: () = assert!(size_of::<usize>() == 8);

fn path() -> Option<PathBuf> {
    let path: PathBuf = std::env::var_os(VARIABLE).filter(|p| !p.is_empty())?.into();
    // `LoadLibraryExW` searches the DLL's own folder only for a full path.
    Some(std::path::absolute(&path).unwrap_or(path))
}

/// Whether the redirect is on: asked for, and the PC DLL loaded with its
/// five functions. Without the DLL the title's own upscaler runs.
pub fn enabled() -> bool {
    api().is_some()
}

/// Whether the title's `file` is answered by the redirect instead of being
/// mapped from the title's folder.
pub fn replaces(file: &str) -> bool {
    is_title_fidelityfx(file) && enabled()
}

/// Whether `file` is one of the title's FidelityFX DLLs.
fn is_title_fidelityfx(file: &str) -> bool {
    DLLS.iter().any(|d| d.eq_ignore_ascii_case(file))
}

// `ffxReturnCode_t` (header).
const OK: u32 = 0;
const ERROR: u32 = 1;
const ERROR_UNKNOWN_DESCTYPE: u32 = 2;
const NO_PROVIDER: u32 = 4;
const ERROR_PARAMETER: u32 = 6;

// Description types and the parts of them (header unless marked).
/// The effect and the backend a type belongs to: types of a backend other
/// than DX12 (0) are the console's (measured: `0x0100_0002`, its backend
/// description, and `0x0100_0003`).
const EFFECT_MASK: u64 = 0x00ff_0000;
const EFFECT_UPSCALE: u64 = 0x0001_0000;
/// `ffxCreateBackendDX12Desc`: the header, then the device (read: the PC
/// DLL finds its device there, at +16, or at +24 of a query of versions).
const CREATE_BACKEND_DX12: u64 = 0x2;
const CREATE_UPSCALE: u64 = 0x1_0000;
const CREATE_UPSCALE_VERSION: u64 = 0x1_000b;
const DISPATCH_UPSCALE: u64 = 0x1_0001;
/// Read: the PC DLL's dispatch takes this and `0x1_0001`. (Neptune-VE had
/// `0x1_0002`, which is a query's type.)
const DISPATCH_GENERATE_REACTIVE_MASK: u64 = 0x1_0006;
/// A version override (`ffxOverrideVersion`: the header, then a provider's
/// id), read: the PC DLL's create and query look for it. (Neptune-VE took 1
/// for it, the global debug callback.)
const OVERRIDE_VERSION: u64 = 5;
/// The global debug callback: 1 read (the PC DLL's configure takes it and
/// key-values), 7 the header's newer one.
const GLOBAL_DEBUG: [u64; 2] = [1, 7];
/// Where `ffxCreateContextDescUpscale` keeps `fpMessage`, after the header,
/// the flags and two sizes.
const CREATE_MESSAGE: usize = 40;

/// Whether a description of type `ty` may go to the PC DLL: of the DX12
/// backend, and no bits above the type's 32.
fn for_the_pc(ty: u64) -> bool {
    ty & !(EFFECT_MASK | 0xffff) == 0
}

/// `upscaler_version(major, minor, patch)` as text.
fn version_text(version: u32) -> String {
    format!("{}.{}.{}", version >> 22, (version >> 12) & 0x3ff, version & 0xfff)
}

type Create = unsafe extern "system" fn(*mut *mut c_void, *mut u8, *const c_void) -> u32;
type Destroy = unsafe extern "system" fn(*mut *mut c_void, *const c_void) -> u32;
type Header = unsafe extern "system" fn(*mut *mut c_void, *mut u8) -> u32;

struct Api {
    create: Create,
    destroy: Destroy,
    configure: Header,
    query: Header,
    dispatch: Header,
}

/// The PC DLL's functions, loaded once.
fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        let path = path()?;
        match load(&path) {
            Ok(api) => {
                println!("fidelityfx     {} (host, for the title's upscaler)", path.display());
                Some(api)
            }
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    %error,
                    "VILOCITY_FSR_DLL: the PC FidelityFX DLL cannot be used; the title's own upscaler runs"
                );
                None
            }
        }
    })
    .as_ref()
}

fn load(path: &Path) -> Result<Api, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW,
    };
    let open = |path: &Path| {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: a NUL-terminated path. The DLL's own folder is searched
        // for what it imports.
        unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_WITH_ALTERED_SEARCH_PATH) }
    };
    // The PC loader loads its effect DLLs by name alone (`LoadLibraryA`):
    // that finds one already loaded wherever it lies, and otherwise looks
    // beside the program, not beside the loader. So the upscaler beside it
    // is loaded first.
    if path.file_name().is_some_and(|f| f.eq_ignore_ascii_case(PC_LOADER)) {
        let upscaler = path.with_file_name(PC_UPSCALER);
        if upscaler.is_file() && open(&upscaler).is_null() {
            return Err(format!(
                "LoadLibrary of {} failed: {}",
                upscaler.display(),
                std::io::Error::last_os_error()
            ));
        }
    }
    let module = open(path);
    if module.is_null() {
        return Err(format!("LoadLibrary failed: {}", std::io::Error::last_os_error()));
    }
    let export = |name: &'static [u8]| {
        // SAFETY: a loaded module and a NUL-terminated name.
        unsafe { GetProcAddress(module, name.as_ptr()) }
            .map(|f| f as usize)
            .ok_or_else(|| format!("no export {}", String::from_utf8_lossy(&name[..name.len() - 1])))
    };
    // SAFETY: the five exports have these signatures in every FidelityFX
    // API DLL (`ffx_api.h`).
    unsafe {
        Ok(Api {
            create: std::mem::transmute::<usize, Create>(export(b"ffxCreateContext\0")?),
            destroy: std::mem::transmute::<usize, Destroy>(export(b"ffxDestroyContext\0")?),
            configure: std::mem::transmute::<usize, Header>(export(b"ffxConfigure\0")?),
            query: std::mem::transmute::<usize, Header>(export(b"ffxQuery\0")?),
            dispatch: std::mem::transmute::<usize, Header>(export(b"ffxDispatch\0")?),
        })
    }
}

/// Title memory changed for one call, put back after it, the last change
/// first.
#[derive(Default)]
struct Patches(Vec<Patch>);

enum Patch {
    Word(*mut usize, usize),
    Dword(*mut u32, u32),
}

impl Patches {
    /// Sets the word at `at` to `new`.
    ///
    /// # Safety
    /// `at` is a writable word of a description the title passed (or of
    /// ours); it need not be aligned.
    unsafe fn set(&mut self, at: *mut usize, new: usize) {
        // SAFETY: as the caller says.
        unsafe {
            self.0.push(Patch::Word(at, at.read_unaligned()));
            at.write_unaligned(new);
        }
    }

    /// Sets the 32 bits at `at` to `new`.
    ///
    /// # Safety
    /// As [`Patches::set`].
    unsafe fn set_dword(&mut self, at: *mut u32, new: u32) {
        // SAFETY: as the caller says.
        unsafe {
            self.0.push(Patch::Dword(at, at.read_unaligned()));
            at.write_unaligned(new);
        }
    }
}

impl Drop for Patches {
    fn drop(&mut self) {
        for patch in self.0.iter().rev() {
            // SAFETY: written by `set` or `set_dword` on the same call.
            unsafe {
                match *patch {
                    Patch::Word(at, old) => at.write_unaligned(old),
                    Patch::Dword(at, old) => at.write_unaligned(old),
                }
            }
        }
    }
}

/// The most descriptions of a chain looked at (a chain the title got wrong
/// may come back to itself).
const CHAIN: usize = 32;

/// A description's type.
///
/// # Safety
/// `node` is a description: its type, then the next one.
unsafe fn kind(node: *const u8) -> u64 {
    // SAFETY: as the caller says.
    unsafe { node.cast::<u64>().read_unaligned() }
}

/// The description after `node`.
///
/// # Safety
/// As [`kind`].
unsafe fn next(node: *const u8) -> *mut u8 {
    // SAFETY: as the caller says.
    unsafe { node.add(8).cast::<*mut u8>().read_unaligned() }
}

/// The descriptions of the chain at `head`, in order.
fn chain(head: *mut u8) -> Vec<*mut u8> {
    let mut nodes = Vec::new();
    let mut node = head;
    while !node.is_null() && nodes.len() < CHAIN {
        nodes.push(node);
        // SAFETY: a description of the chain the title passed.
        node = unsafe { next(node) };
    }
    nodes
}

/// Leaves the descriptions after `head` for which `out` says so out of the
/// chain (for this call); the types left out.
fn leave_out(patches: &mut Patches, head: *mut u8, mut out: impl FnMut(*mut u8, u64) -> bool) -> Vec<u64> {
    let mut left = Vec::new();
    let mut kept = head;
    for node in chain(head).into_iter().skip(1) {
        // SAFETY: a description of the chain.
        let ty = unsafe { kind(node) };
        if out(node, ty) {
            // SAFETY: the next-description word of the one kept before it.
            unsafe { patches.set(kept.add(8).cast(), next(node) as usize) };
            left.push(ty);
        } else {
            kept = node;
        }
    }
    left
}

/// Logs the description types a call carries, once per type: what the
/// title sends is not documented for the console build.
fn note(call: &str, head: *mut u8) {
    static SEEN: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
    for node in chain(head) {
        // SAFETY: a description of the chain.
        let ty = unsafe { kind(node) };
        let fresh = {
            let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
            let key = (call.to_owned(), ty);
            let fresh = !seen.contains(&key);
            if fresh {
                seen.push(key);
            }
            fresh
        };
        if fresh {
            // SAFETY: the first 12 words of a description, as a dump to
            // learn its layout from.
            let words: Vec<String> = (0..12)
                .map(|i| format!("{:#x}", unsafe { node.cast::<u64>().add(i).read_unaligned() }))
                .collect();
            tracing::info!(call, ty = format_args!("{ty:#x}"), ?words, "FidelityFX description");
        }
    }
}

/// Logs `what` once per `key`.
fn once(key: (&'static str, u64), what: impl FnOnce()) {
    static SAID: Mutex<Vec<(&'static str, u64)>> = Mutex::new(Vec::new());
    let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    if !said.contains(&key) {
        said.push(key);
        drop(said);
        what();
    }
}

/// Puts `host` where the words `words` (byte offsets) of description
/// `node` hold the title's device.
fn swap_device(patches: &mut Patches, node: *mut u8, words: std::ops::Range<usize>, host: *mut c_void) {
    for at in words.step_by(8) {
        // SAFETY: inside (or just past) a description the title passed:
        // read, and written only where it holds the title's device.
        unsafe {
            let word = node.add(at).cast::<usize>();
            if external::is_title_device(word.read_unaligned() as *const c_void) {
                patches.set(word, host as usize);
            }
        }
    }
}

/// What the PC DLL says (`ffxApiMessage`: a type, 0 an error and 1 a
/// warning as recalled, and a NUL-terminated UTF-16 text), in our log. It
/// is given this in place of the title's callback, which is console code.
extern "system" fn message(kind: u32, text: *const u16) {
    const MOST: u64 = 200;
    static SAID: AtomicU64 = AtomicU64::new(0);
    match SAID.fetch_add(1, Ordering::Relaxed) {
        n if n < MOST => tracing::warn!(kind, "FidelityFX (PC DLL): {}", wide(text)),
        MOST => tracing::warn!("FidelityFX (PC DLL): more messages are not logged"),
        _ => {}
    }
}

/// A NUL-terminated UTF-16 text, up to a bound.
fn wide(text: *const u16) -> String {
    let mut units = Vec::new();
    while !text.is_null() && units.len() < 2048 {
        // SAFETY: inside the text, which ends at a zero.
        let unit = unsafe { text.add(units.len()).read_unaligned() };
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16_lossy(&units)
}

/// The chain of a context's creation, made right for the PC DLL: the
/// console's descriptions and version overrides out; the title's DX12
/// backend description, if it has one, names `device`, or `backend` (ours,
/// naming it) is put at the end; the PC DLL's messages come to us. The
/// title's upscaler version description, if there is one.
fn prepare_create(patches: &mut Patches, desc: *mut u8, device: *mut c_void, backend: &mut [u64; 3]) -> Option<*mut u8> {
    let left = leave_out(patches, desc, |_, ty| !for_the_pc(ty) || ty == OVERRIDE_VERSION);
    if !left.is_empty() {
        let left: Vec<String> = left.iter().map(|ty| format!("{ty:#x}")).collect();
        tracing::info!(?left, "FidelityFX create: the console's descriptions are not given to the PC DLL");
    }
    *backend = [CREATE_BACKEND_DX12, 0, device as u64];
    let mut has_backend = false;
    let mut version = None;
    for node in chain(desc) {
        // SAFETY: a description of the chain.
        match unsafe { kind(node) } {
            CREATE_BACKEND_DX12 => {
                has_backend = true;
                swap_device(patches, node, 16..24, device);
            }
            // SAFETY: the message callback's word of the upscaler's
            // creation description.
            CREATE_UPSCALE => unsafe { patches.set(node.add(CREATE_MESSAGE).cast(), message as *const () as usize) },
            CREATE_UPSCALE_VERSION => version = Some(node),
            _ => {}
        }
    }
    if !has_backend && let Some(&last) = chain(desc).last() {
        // SAFETY: the last description's next-description word; `backend`
        // lives through the call.
        unsafe { patches.set(last.add(8).cast(), backend.as_mut_ptr() as usize) };
    }
    version
}

/// `ffxCreateContext(context, desc, allocation callbacks)`: the upscaler's
/// context, made by the PC DLL on the host device. The title's allocation
/// callbacks are not passed on (the PC DLL allocates itself).
///
/// The title's chain names the upscaler version it was built against
/// (measured: its DLL wants one, 4.0.0 to 4.1.1). The PC upscaler 4.1.0's
/// create takes 4.0.0 to 4.1.0 and refuses any other; without the
/// description it takes 4.0.0 and says so (read). So a creation that fails
/// is tried again with the title's version at its minor version's patch 0,
/// then without the description, as for a program from before it.
extern "system" fn ffxCreateContext(context: *mut *mut c_void, desc: *mut u8, _alloc: *const c_void) -> u32 {
    let Some(api) = api() else { return ERROR };
    if context.is_null() || desc.is_null() {
        return ERROR_PARAMETER;
    }
    let Some(device) = external::host_device() else { return ERROR };
    note("create", desc);
    // SAFETY: a title description: its type first.
    let ty = unsafe { kind(desc) };
    if ty & EFFECT_MASK != EFFECT_UPSCALE || !for_the_pc(ty) {
        once(("create", ty), || {
            tracing::warn!(ty = format_args!("{ty:#x}"), "FidelityFX: only the upscaler's contexts are made by the PC DLL")
        });
        return NO_PROVIDER;
    }
    let mut patches = Patches::default();
    let mut backend = [0u64; 3];
    let version = prepare_create(&mut patches, desc, device, &mut backend);
    let code = create_trying_versions(&mut patches, desc, version, || {
        // SAFETY: the PC DLL's export, with a chain the title wrote and we
        // patched.
        unsafe { (api.create)(context, desc, std::ptr::null()) }
    });
    drop(patches);
    if code == OK {
        tracing::info!("ffxCreateContext on the PC DLL");
    } else {
        tracing::warn!(code, "ffxCreateContext on the PC DLL");
    }
    code
}

/// `create`'s code with the chain at `desc` as it is, and if that fails and
/// the chain has the upscaler version description `version`: with the
/// version at its minor version's patch 0, then without the description.
fn create_trying_versions(
    patches: &mut Patches,
    desc: *mut u8,
    version: Option<*mut u8>,
    mut create: impl FnMut() -> u32,
) -> u32 {
    let mut code = create();
    let Some(node) = version else { return code };
    // SAFETY: the version word of the upscaler version description.
    let word = unsafe { node.add(16).cast::<u32>() };
    // SAFETY: as above.
    let asked = unsafe { word.read_unaligned() };
    let patch_0 = asked & !0xfff;
    if code != OK && patch_0 != asked {
        tracing::warn!(
            code,
            version = version_text(asked),
            "ffxCreateContext on the PC DLL refused the title's upscaler version; again at {}",
            version_text(patch_0)
        );
        // SAFETY: as above.
        unsafe { patches.set_dword(word, patch_0) };
        code = create();
    }
    if code != OK {
        tracing::warn!(
            code,
            version = version_text(asked),
            "ffxCreateContext on the PC DLL refused the title's upscaler version; again without it"
        );
        leave_out(patches, desc, |n, _| n == node);
        code = create();
    }
    code
}

extern "system" fn ffxDestroyContext(context: *mut *mut c_void, _alloc: *const c_void) -> u32 {
    let Some(api) = api() else { return ERROR };
    // SAFETY: the PC DLL's export, with what its create made.
    unsafe { (api.destroy)(context, std::ptr::null()) }
}

/// A configuration or query: the console's descriptions and a version
/// override out, the host device where the title names its own (a query of
/// versions does).
fn with_host_device(f: Header, context: *mut *mut c_void, desc: *mut u8) -> u32 {
    let Some(device) = external::host_device() else { return ERROR };
    let mut patches = Patches::default();
    leave_out(&mut patches, desc, |_, ty| !for_the_pc(ty) || ty == OVERRIDE_VERSION);
    for node in chain(desc) {
        swap_device(&mut patches, node, 16..48, device);
    }
    // SAFETY: the PC DLL's export, with a chain the title wrote and we
    // patched.
    unsafe { f(context, desc) }
}

/// The title's global debug callback is console code: it is not given to
/// the PC DLL, whose messages about a context come to us anyway
/// ([`message`]).
extern "system" fn ffxConfigure(context: *mut *mut c_void, desc: *mut u8) -> u32 {
    let Some(api) = api() else { return ERROR };
    if desc.is_null() {
        return ERROR_PARAMETER;
    }
    note("configure", desc);
    // SAFETY: a title description: its type first.
    let ty = unsafe { kind(desc) };
    if GLOBAL_DEBUG.contains(&ty) {
        once(("configure", ty), || {
            tracing::info!(ty, "FidelityFX: the title's debug callback is not given to the PC DLL")
        });
        return OK;
    }
    with_host_device(api.configure, context, desc)
}

extern "system" fn ffxQuery(context: *mut *mut c_void, desc: *mut u8) -> u32 {
    let Some(api) = api() else { return ERROR };
    if desc.is_null() {
        return ERROR_PARAMETER;
    }
    note("query", desc);
    with_host_device(api.query, context, desc)
}

/// Where a dispatch description keeps its command list, and its resources:
/// each `FfxApiResource` is a pointer, a 32-byte description (type,
/// format, width, height, depth, levels, flags, usage) and a state, 48
/// bytes (header).
const COMMAND_LIST: usize = 16;
const RESOURCE_SIZE: usize = 48;
const RESOURCE_FORMAT: usize = 12;
const RESOURCE_WIDTH: usize = 16;
const RESOURCE_HEIGHT: usize = 20;
const RESOURCE_USAGE: usize = 36;
const RESOURCE_STATE: usize = 40;
// `FfxApiResourceState` and `FfxApiResourceUsage` bits (header). Read: the
// PC DLL's backend takes these two states for `UNORDERED_ACCESS` and
// `NON_PIXEL_SHADER_RESOURCE` (`vilocity_gpu::external::WRITE`, `READ`).
const STATE_UNORDERED_ACCESS: u32 = 1 << 1;
const STATE_COMPUTE_READ: u32 = 1 << 2;
const USAGE_UAV: u32 = 1 << 1;

/// The DXGI formats of the FidelityFX surface formats 0 to 39, as the PC
/// DLL turns them (read: its backend's table). Its views of a texture are
/// made in the texture's own format, and only for R8G8B8A8 (UNORM or sRGB)
/// in the format its description names; a depth texture is viewed in the
/// format of the plane shaders read: R32G8X24 (19, 20) as
/// R32_FLOAT_X8X24_TYPELESS, D32 as R32_FLOAT, R24G8 (44, 45) as
/// R24_UNORM_X8_TYPELESS, D16 as R16_UNORM (read: its backend's resource
/// registration and view format tables).
const SURFACE_FORMATS: [u32; 40] = [
    0, 1, 3, 2, 10, 6, 16, 62, 42, 27, 28, 31, 29, 90, 87, 91, 26, 24, 34, 36, 38, 54, 57, 56, 58, 61, 49, 50,
    41, 67, 9, 15, 23, 33, 53, 60, 48, 39, 17, 63,
];

/// The DXGI format of FidelityFX surface format `surface`, if the PC DLL
/// knows it.
fn dxgi_format(surface: u32) -> Option<u32> {
    SURFACE_FORMATS.get(surface as usize).copied().filter(|&f| f != 0)
}

/// The FidelityFX surface format of DXGI format `dxgi`, if it has one.
fn surface_format(dxgi: u32) -> Option<u32> {
    SURFACE_FORMATS.iter().position(|&f| f == dxgi && f != 0).map(|i| i as u32)
}

/// The surface format the PC DLL is told for a texture the title describes
/// as `described` and whose host copy is in DXGI format `host`: the host
/// copy's own where the title's names another family and the host's has a
/// surface format, as for a stand-in (an R9G9B9E5 target is R11G11B10 or
/// R16G16B16A16 on the host), so that the description is of the texture
/// given. `None` leaves the title's: it names the host copy's family, or the
/// host's format has no surface format, as the depth-stencil ones (the
/// title's D32S8 depth, R32G8X24_TYPELESS on the host, described as
/// R32_FLOAT); the DLL makes its views of those in the texture's own format
/// anyway, a depth texture's in its depth plane's ([`SURFACE_FORMATS`]).
/// shortcut: a D16 depth texture that shaders read is R16_TYPELESS on the
/// host, which the DLL views as R16_FLOAT, not R16_UNORM: its depths would
/// be read wrong. The title's depth is D32S8.
fn told_format(described: u32, host: u32) -> Option<u32> {
    if dxgi_format(described).is_some_and(|d| external::same_family(d, host)) {
        return None;
    }
    surface_format(host)
}

/// The rest of `ffxDispatchDescUpscale` (header; read: the PC DLL's
/// dispatch takes them from these places).
const JITTER: usize = 360;
const MOTION_VECTOR_SCALE: usize = 368;
const RENDER_SIZE: usize = 376;
const UPSCALE_SIZE: usize = 384;
const SHARPENING: usize = 392;
const SHARPNESS: usize = 396;
const FRAME_TIME: usize = 400;
const PRE_EXPOSURE: usize = 404;
const RESET: usize = 408;
const CAMERA: usize = 412;
const FLAGS: usize = 428;

/// A dispatch's resources: where they are, whether the effect writes them,
/// and what they are.
type Slots = [(usize, bool, &'static str)];

/// `ffxDispatchDescUpscale` (header).
const UPSCALE: [(usize, bool, &str); 7] = [
    (24, false, "colour"),
    (24 + RESOURCE_SIZE, false, "depth"),
    (24 + 2 * RESOURCE_SIZE, false, "motion vectors"),
    (24 + 3 * RESOURCE_SIZE, false, "exposure"),
    (24 + 4 * RESOURCE_SIZE, false, "reactive mask"),
    (24 + 5 * RESOURCE_SIZE, false, "transparency and composition"),
    (24 + 6 * RESOURCE_SIZE, true, "output"),
];
/// `ffxDispatchDescUpscaleGenerateReactiveMask` (recalled).
const REACTIVE_MASK: [(usize, bool, &str); 3] = [
    (24, false, "opaque colour"),
    (24 + RESOURCE_SIZE, false, "colour before upscaling"),
    (24 + 2 * RESOURCE_SIZE, true, "reactive mask"),
];

/// Whether description `node` names the title's device, a texture of the
/// title's or `list` in its first words (after the header).
fn names_title_objects(node: *mut u8, list: *const c_void) -> bool {
    (16..48).step_by(8).any(|at| {
        // SAFETY: inside (or just past) a description the title passed.
        let word = unsafe { node.add(at).cast::<usize>().read_unaligned() } as *const c_void;
        !word.is_null() && (word == list || external::is_title_device(word) || external::is_title_texture(word))
    })
}

/// What became of a dispatch on the host list.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Done {
    /// The PC DLL took it and said this.
    Ran(u32),
    /// It was not given to the PC DLL: a texture (which) could not be (why).
    Refused(&'static str, &'static str),
}

/// What the fallback needs of an upscale dispatch ([`fall_back`]), read
/// before anything of it is patched.
struct Upscale {
    colour: *const c_void,
    output: *const c_void,
    render: [u32; 2],
    upscale: [u32; 2],
}

impl Upscale {
    fn of(desc: *mut u8) -> Upscale {
        // SAFETY: words of an upscale dispatch description.
        let word = |at: usize| unsafe { desc.add(at).cast::<usize>().read_unaligned() } as *const c_void;
        // SAFETY: as above.
        let size = |at: usize| [0, 4].map(|i| unsafe { desc.add(at + i).cast::<u32>().read_unaligned() });
        Upscale {
            colour: word(UPSCALE[0].0),
            output: word(UPSCALE[6].0),
            render: size(RENDER_SIZE),
            upscale: size(UPSCALE_SIZE),
        }
    }
}

/// `ffxDispatch(context, desc)`: the title's command list and textures
/// become the host's for the call. Each texture is put in the state the PC
/// DLL is told it is in (compute read, or unordered access for what it
/// writes), whatever the title declared for its console texture: the host
/// list's own tracking knows the host texture's state, and the DLL puts
/// each back into the state it was told of.
///
/// An upscale the PC DLL does not do (refused here, or failed there) still
/// leaves the title a picture: its colour, scaled into the output by a pass
/// of ours ([`fall_back`]). The title is then told the dispatch went well,
/// so that it goes on as with a frame upscaled.
extern "system" fn ffxDispatch(context: *mut *mut c_void, desc: *mut u8) -> u32 {
    let Some(api) = api() else { return ERROR };
    if desc.is_null() {
        return ERROR_PARAMETER;
    }
    note("dispatch", desc);
    // SAFETY: a title description: its type first.
    let ty = unsafe { kind(desc) };
    let slots: &Slots = match ty {
        DISPATCH_UPSCALE => &UPSCALE,
        DISPATCH_GENERATE_REACTIVE_MASK => &REACTIVE_MASK,
        _ => {
            // It names the title's command list, and perhaps its textures:
            // not something to give the PC DLL as it is.
            once(("dispatch", ty), || {
                tracing::warn!(ty = format_args!("{ty:#x}"), "FidelityFX: a dispatch of this kind is not given to the PC DLL")
            });
            return ERROR_UNKNOWN_DESCTYPE;
        }
    };
    if ty == DISPATCH_UPSCALE {
        once(("dispatch fields", ty), || {
            tracing::info!("FidelityFX upscale as the title passes it: {}", upscale_fields(desc))
        });
    }
    // SAFETY: the command list's word of a dispatch description.
    let list = unsafe { desc.add(COMMAND_LIST).cast::<usize>().read_unaligned() } as *const c_void;
    let upscale = (ty == DISPATCH_UPSCALE).then(|| Upscale::of(desc));
    let mut done = Done::Refused("", "nothing recorded");
    let mut fell_back = false;
    let mut patches = Patches::default();
    // More descriptions in the chain that name console objects: left out.
    for ty in leave_out(&mut patches, desc, |node, ty| !for_the_pc(ty) || names_title_objects(node, list)) {
        once(("dispatch extension", ty), || {
            tracing::warn!(ty = format_args!("{ty:#x}"), "FidelityFX dispatch: a description naming console objects is not given to the PC DLL")
        });
    }
    let recorded = external::record(list, |pass| {
        done = hand_over(pass, api, context, desc, ty, slots, &mut patches);
        if let Some(upscale) = &upscale
            && done != Done::Ran(OK)
        {
            fell_back = fall_back(pass, upscale);
        }
    });
    drop(patches);
    if !recorded {
        once(("dispatch list", 0), || {
            tracing::warn!("FidelityFX dispatch refused: the command list is not a list of ours open for recording")
        });
        return ERROR_PARAMETER;
    }
    if done != Done::Ran(OK) {
        failed(ty, desc, slots, done, fell_back);
    }
    match done {
        _ if fell_back => OK,
        Done::Ran(code) => code,
        Done::Refused(..) => ERROR_PARAMETER,
    }
}

/// Gives the dispatch at `desc` to the PC DLL on `pass`'s list. Every
/// texture is looked at first, so that a dispatch refused gets nothing of
/// its textures ready; then each is made ready and described to the DLL as
/// the host has it: its host resource, the state it is put in, whether it
/// allows unordered access, and its format where the title's names another
/// family.
fn hand_over(
    pass: &mut external::Pass,
    api: &Api,
    context: *mut *mut c_void,
    desc: *mut u8,
    ty: u64,
    slots: &Slots,
    patches: &mut Patches,
) -> Done {
    // Each texture as the host has it, said once for each (all of them, also
    // when one is refused).
    let looked: Vec<_> = slots
        .iter()
        .filter_map(|&(at, write, name)| {
            // SAFETY: a resource of the dispatch description.
            let resource = unsafe { desc.add(at) };
            // SAFETY: its pointer.
            let title = unsafe { resource.cast::<usize>().read_unaligned() } as *const c_void;
            if title.is_null() {
                return None;
            }
            let looked = pass.look(title);
            once((name, ty | 1 << 32), || describe(name, resource, title, looked.as_ref().ok()));
            Some((resource, write, name, title, looked))
        })
        .collect();
    let mut handed = Vec::with_capacity(looked.len());
    for (resource, write, name, title, looked) in looked {
        let decided = looked.and_then(|t| {
            if write && !t.unordered() {
                return Err("written, and its host copy allows no unordered access");
            }
            if !write && !t.readable() {
                return Err("read, and its host copy denies shader reads");
            }
            // SAFETY: the format word of the resource's description.
            let described = unsafe { resource.add(RESOURCE_FORMAT).cast::<u32>().read_unaligned() };
            Ok((t, told_format(described, t.format)))
        });
        match decided {
            Ok((t, format)) => handed.push((resource, write, name, title, t, format)),
            Err(why) => return Done::Refused(name, why),
        }
    }
    // SAFETY: the command list's word of the dispatch description.
    unsafe { patches.set(desc.add(COMMAND_LIST).cast(), pass.command_list() as usize) };
    for (resource, write, name, title, t, format) in handed {
        let Some(host) = pass.ready(title, write) else {
            return Done::Refused(name, "it lost its host copy");
        };
        // SAFETY: the words of a resource of the dispatch description.
        unsafe {
            patches.set(resource.cast(), host as usize);
            let state = if write { STATE_UNORDERED_ACCESS } else { STATE_COMPUTE_READ };
            patches.set_dword(resource.add(RESOURCE_STATE).cast(), state);
            // The DLL makes an unordered access view of a texture its
            // description says allows one: the console's may, its host
            // copy may not (depth).
            let usage = resource.add(RESOURCE_USAGE).cast::<u32>();
            let was = usage.read_unaligned();
            let now = match (t.unordered(), write) {
                (false, _) => was & !USAGE_UAV,
                (true, true) => was | USAGE_UAV,
                (true, false) => was,
            };
            if now != was {
                patches.set_dword(usage, now);
            }
            if let Some(format) = format {
                patches.set_dword(resource.add(RESOURCE_FORMAT).cast(), format);
            }
        }
    }
    // SAFETY: the PC DLL's export, with a description the title wrote and
    // we patched, recording onto the host list.
    Done::Ran(unsafe { (api.dispatch)(context, desc) })
}

/// The title's colour scaled into its output by a pass of ours, for a frame
/// the PC DLL did not upscale: whether it was recorded (said once either
/// way).
fn fall_back(pass: &mut external::Pass, u: &Upscale) -> bool {
    match pass.scale(u.colour, u.render, u.output, u.upscale) {
        Ok(()) => {
            once(("fallback", 0), || {
                tracing::warn!(
                    "FidelityFX: a frame the PC DLL did not upscale gets the title's colour, scaled into the output by a pass of ours (said once)"
                )
            });
            true
        }
        Err(why) => {
            once(("fallback", 1), || {
                tracing::warn!(why, "FidelityFX: no picture of ours for a frame the PC DLL did not upscale (said once)")
            });
            false
        }
    }
}

/// Says, for the first dispatches the PC DLL did not do, why and what the
/// title passed.
fn failed(ty: u64, desc: *mut u8, slots: &Slots, done: Done, fell_back: bool) {
    const MOST: u64 = 10;
    static SAID: AtomicU64 = AtomicU64::new(0);
    let n = SAID.fetch_add(1, Ordering::Relaxed);
    if n >= MOST {
        return;
    }
    let why = match done {
        Done::Ran(code) => format!("the PC DLL returned {code}"),
        Done::Refused(texture, why) => format!("not given to the PC DLL: {texture}: {why}"),
    };
    let textures: Vec<String> = slots
        .iter()
        .map(|&(at, _, name)| {
            // SAFETY: a resource's pointer in the dispatch description.
            let title = unsafe { desc.add(at).cast::<usize>().read_unaligned() };
            if title == 0 { format!("{name} none") } else { format!("{name} {title:#x}") }
        })
        .collect();
    let fields = if ty == DISPATCH_UPSCALE { upscale_fields(desc) } else { String::new() };
    tracing::warn!(
        ty = format_args!("{ty:#x}"),
        fell_back,
        "FidelityFX dispatch {} of the first {MOST} not done: {why}; {fields}; textures: {}",
        n + 1,
        textures.join(", ")
    );
}

/// What an upscale dispatch passes besides its textures, as text.
fn upscale_fields(desc: *mut u8) -> String {
    // SAFETY: words of an upscale dispatch description (432 bytes).
    let word = |at: usize| unsafe { desc.add(at).cast::<u32>().read_unaligned() };
    let float = |at: usize| f32::from_bits(word(at));
    // SAFETY: as above.
    let byte = |at: usize| unsafe { desc.add(at).read() };
    format!(
        "render {}x{}, upscale {}x{}, jitter ({}, {}), motion vector scale ({}, {}), sharpening {} at {}, frame time {} ms, pre-exposure {}, reset {}, camera near {} far {} vertical field {} rad, {} m a unit, flags {:#x}",
        word(RENDER_SIZE),
        word(RENDER_SIZE + 4),
        word(UPSCALE_SIZE),
        word(UPSCALE_SIZE + 4),
        float(JITTER),
        float(JITTER + 4),
        float(MOTION_VECTOR_SCALE),
        float(MOTION_VECTOR_SCALE + 4),
        byte(SHARPENING),
        float(SHARPNESS),
        float(FRAME_TIME),
        float(PRE_EXPOSURE),
        byte(RESET),
        float(CAMERA),
        float(CAMERA + 4),
        float(CAMERA + 8),
        float(CAMERA + 12),
        word(FLAGS),
    )
}

/// Says, once per texture of a dispatch, what the title described, what the
/// title made it as and what the host has (`t`; `None`: no texture of the
/// title's with a host copy): the PC DLL makes its views from the
/// description.
fn describe(name: &str, resource: *mut u8, title: *const c_void, t: Option<&external::Texture>) {
    // SAFETY: the 32-byte description and the state of a resource of the
    // dispatch description.
    let words: Vec<u32> = (2..11).map(|i| unsafe { resource.cast::<u32>().add(i).read_unaligned() }).collect();
    let described = format!(
        "format {} (DXGI {}), {}x{}, usage {:#x}, state {:#x}",
        words[1],
        dxgi_format(words[1]).unwrap_or(0),
        words[2],
        words[3],
        words[7],
        words[8]
    );
    let Some(t) = t else {
        tracing::info!(texture = name, title = format_args!("{title:p}"), described, "FidelityFX dispatch: a texture the host has no copy of");
        return;
    };
    let (own_format, own_width, own_height) = t.title.unwrap_or_default();
    tracing::info!(
        texture = name,
        title = format_args!("{title:p}"),
        described,
        all_described = ?words,
        title_format = own_format,
        title_size = format_args!("{own_width}x{own_height}"),
        host_format = t.format,
        host_size = format_args!("{}x{}", t.width, t.height),
        host_levels = t.levels,
        host_flags = format_args!("{:#x}", t.flags),
        stand_in = t.stand_in,
        told = ?told_format(words[1], t.format),
        "FidelityFX dispatch: a texture of the title's (described: type, format, width, height, depth, levels, flags, usage, state; told: the surface format the PC DLL is told in its place)"
    );
    // SAFETY: the width and height words of the description.
    let (width, height) = unsafe {
        (
            resource.add(RESOURCE_WIDTH).cast::<u32>().read_unaligned(),
            resource.add(RESOURCE_HEIGHT).cast::<u32>().read_unaligned(),
        )
    };
    if (u64::from(width), height) != (t.width, t.height) {
        tracing::warn!(
            texture = name,
            described = format_args!("{width}x{height}"),
            host = format_args!("{}x{}", t.width, t.height),
            "FidelityFX dispatch: a texture's host copy is not the size the title describes"
        );
    }
}

pub(crate) fn module() -> HleModule {
    HleModule {
        name: "fidelityfx",
        dlls: &DLLS,
        exports: vec![
            crate::hle_export!(ffxCreateContext),
            crate::hle_export!(ffxDestroyContext),
            crate::hle_export!(ffxConfigure),
            crate::hle_export!(ffxQuery),
            crate::hle_export!(ffxDispatch),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn the_title_s_fidelityfx_dlls_are_answered_for() {
        for file in DLLS {
            assert!(is_title_fidelityfx(file));
        }
        assert!(is_title_fidelityfx("AMD_FidelityFX_Loader_Xbox.dll"));
        for file in [PC_LOADER, PC_UPSCALER, "amd_fidelityfx_dx12.dll", "kernel32.dll", "amd_fidelityfx_loader_xbox"] {
            assert!(!is_title_fidelityfx(file), "{file}");
        }
    }

    #[test]
    fn without_the_variable_nothing_is_replaced() {
        if std::env::var_os(VARIABLE).is_some() {
            return;
        }
        assert!(!enabled());
        assert!(!replaces("amd_fidelityfx_loader_xbox.dll"));
        // No module of ours answers for the title's DLLs: they are mapped
        // from its folder, as before.
        for file in DLLS {
            assert!(!crate::registry().iter().any(|m| m.serves(file)), "{file}");
            let name: Vec<u16> = file.encode_utf16().chain([0]).collect();
            assert!(super::super::kernel::GetModuleHandleW(name.as_ptr()).is_null(), "{file}");
        }
    }

    #[test]
    fn patches_are_put_back() {
        let mut words = [1usize, 2];
        let mut dwords = [3u32, 4];
        {
            let mut p = Patches::default();
            // SAFETY: words of local arrays.
            unsafe {
                p.set(&mut words[0], 9);
                p.set(&mut words[0], 10);
                p.set(&mut words[1], 7);
                p.set_dword(&mut dwords[1], 8);
                p.set_dword(&mut dwords[1], 5);
            }
            assert_eq!(words, [10, 7]);
            assert_eq!(dwords, [3, 5]);
        }
        assert_eq!(words, [1, 2]);
        assert_eq!(dwords, [3, 4]);
    }

    /// A description of a chain: its type, the next one, then words.
    /// (Read through pointers only, as the DLL reads them.)
    #[repr(C)]
    #[allow(dead_code)]
    struct Node {
        ty: u64,
        next: *mut Node,
        rest: [u64; 7],
    }

    fn nodes(types: &[u64]) -> Vec<Node> {
        let mut nodes: Vec<Node> = types
            .iter()
            .map(|&ty| Node { ty, next: std::ptr::null_mut(), rest: [0; 7] })
            .collect();
        for i in 1..nodes.len() {
            let next = &raw mut nodes[i];
            nodes[i - 1].next = next;
        }
        nodes
    }

    fn types(head: *mut Node) -> Vec<u64> {
        // SAFETY: a chain of `Node`s.
        chain(head.cast()).into_iter().map(|n| unsafe { kind(n) }).collect()
    }

    #[test]
    fn the_console_s_descriptions_are_left_out_for_the_call() {
        let mut descs = nodes(&[CREATE_UPSCALE, CREATE_UPSCALE_VERSION, 0x0100_0002, 0x0100_0003, 5]);
        let head = &raw mut descs[0];
        let all = types(head);
        {
            let mut patches = Patches::default();
            let left = leave_out(&mut patches, head.cast(), |_, ty| !for_the_pc(ty) || ty == OVERRIDE_VERSION);
            assert_eq!(left, [0x0100_0002, 0x0100_0003, 5]);
            assert_eq!(types(head), [CREATE_UPSCALE, CREATE_UPSCALE_VERSION]);
        }
        assert_eq!(types(head), all);
    }

    #[test]
    fn a_creation_gets_the_host_device_and_our_messages() {
        let mut descs = nodes(&[CREATE_UPSCALE, CREATE_UPSCALE_VERSION, 0x0100_0002]);
        let head = &raw mut descs[0];
        let device = 0x1234_5678usize as *mut c_void;
        // `fpMessage` is the sixth word of the upscaler's description.
        let message_word = (CREATE_MESSAGE - 16) / 8;
        let mut backend = [0u64; 3];
        {
            let mut patches = Patches::default();
            let version = prepare_create(&mut patches, head.cast(), device, &mut backend);
            assert_eq!(version, Some((&raw mut descs[1]).cast()));
            assert_eq!(types(head), [CREATE_UPSCALE, CREATE_UPSCALE_VERSION, CREATE_BACKEND_DX12]);
            assert_eq!(backend, [CREATE_BACKEND_DX12, 0, device as u64]);
            assert_eq!(descs[0].rest[message_word], message as *const () as usize as u64);
        }
        assert_eq!(types(head), [CREATE_UPSCALE, CREATE_UPSCALE_VERSION, 0x0100_0002]);
        assert_eq!(descs[0].rest[message_word], 0);
    }

    /// What the PC upscaler 4.1.0's create does with a version description
    /// (read from its code): 4.0.0 to 4.1.0 taken, others refused, none
    /// taken as 4.0.0.
    fn pc_upscaler_4_1_0(head: *mut Node) -> u32 {
        let found = chain(head.cast()).into_iter().find(|&n| unsafe { kind(n) } == CREATE_UPSCALE_VERSION);
        match found.map(|n| unsafe { n.add(16).cast::<u32>().read_unaligned() }) {
            Some(v) if !(0x0100_0000..=0x0100_1000).contains(&v) => ERROR_PARAMETER,
            _ => OK,
        }
    }

    #[test]
    fn a_creation_tries_the_title_s_version_then_its_patch_0_then_none() {
        let version = |major: u32, minor: u32, patch: u32| u64::from((major << 22) | (minor << 12) | patch);
        // The title's 4.1.1: taken at 4.1.0, and put back after the call.
        let mut descs = nodes(&[CREATE_UPSCALE, CREATE_UPSCALE_VERSION]);
        descs[1].rest[0] = version(4, 1, 1);
        let head = &raw mut descs[0];
        let node = Some((&raw mut descs[1]).cast());
        let mut tried = Vec::new();
        {
            let mut patches = Patches::default();
            let code = create_trying_versions(&mut patches, head.cast(), node, || {
                // SAFETY: the version word of our second description.
                tried.push(unsafe { (*head.add(1)).rest[0] });
                pc_upscaler_4_1_0(head)
            });
            assert_eq!(code, OK);
        }
        assert_eq!(tried, [version(4, 1, 1), version(4, 1, 0)]);
        assert_eq!(descs[1].rest[0], version(4, 1, 1));
        // A version no lower patch helps (4.2.0): tried without the
        // description, which is in the chain again after the call.
        descs[1].rest[0] = version(4, 2, 0);
        let mut calls = 0;
        {
            let mut patches = Patches::default();
            let code = create_trying_versions(&mut patches, head.cast(), node, || {
                calls += 1;
                pc_upscaler_4_1_0(head)
            });
            assert_eq!(code, OK);
            assert_eq!(types(head), [CREATE_UPSCALE]);
        }
        assert_eq!(calls, 2);
        assert_eq!(types(head), [CREATE_UPSCALE, CREATE_UPSCALE_VERSION]);
        // Taken at once: one call.
        descs[1].rest[0] = version(4, 0, 2);
        let mut patches = Patches::default();
        let mut calls = 0;
        create_trying_versions(&mut patches, head.cast(), node, || {
            calls += 1;
            pc_upscaler_4_1_0(head)
        });
        assert_eq!(calls, 1);
    }

    #[test]
    fn only_dx12_descriptions_go_to_the_pc() {
        for ty in [CREATE_BACKEND_DX12, CREATE_UPSCALE, CREATE_UPSCALE_VERSION, DISPATCH_UPSCALE, 4, 7] {
            assert!(for_the_pc(ty), "{ty:#x}");
        }
        for ty in [0x0100_0002, 0x0100_0003, 0x0200_0002, 1 << 40] {
            assert!(!for_the_pc(ty), "{ty:#x}");
        }
    }

    #[test]
    fn versions_read_as_the_sdk_writes_them() {
        let version = |major: u32, minor: u32, patch: u32| (major << 22) | (minor << 12) | patch;
        assert_eq!(version_text(version(4, 1, 1)), "4.1.1");
        assert_eq!(version_text(version(3, 1, 5)), "3.1.5");
    }

    #[test]
    fn a_message_is_read_to_its_end() {
        let text: Vec<u16> = "FSR: ok\0ignored".encode_utf16().collect();
        assert_eq!(wide(text.as_ptr()), "FSR: ok");
        assert_eq!(wide(std::ptr::null()), "");
    }

    /// The C layouts the offsets above are taken from (`ffx_api_types.h`,
    /// `ffx_upscale.h`), laid out by the same rules. (Only measured.)
    #[repr(C)]
    #[allow(dead_code)]
    struct Resource {
        resource: *mut c_void,
        description: [u32; 8],
        state: u32,
    }

    #[repr(C)]
    #[allow(dead_code)]
    struct DispatchUpscale {
        ty: u64,
        next: *mut c_void,
        command_list: *mut c_void,
        resources: [Resource; 7],
        jitter: [f32; 2],
        motion_vector_scale: [f32; 2],
        render_size: [u32; 2],
        upscale_size: [u32; 2],
        sharpening: bool,
        sharpness: f32,
        frame_time: f32,
        pre_exposure: f32,
        reset: bool,
        camera: [f32; 4],
        flags: u32,
    }

    #[repr(C)]
    #[allow(dead_code)]
    struct CreateUpscale {
        ty: u64,
        next: *mut c_void,
        flags: u32,
        max_render_size: [u32; 2],
        max_upscale_size: [u32; 2],
        message: *const c_void,
    }

    #[test]
    fn offsets_are_those_of_the_c_structures() {
        assert_eq!(size_of::<Resource>(), RESOURCE_SIZE);
        assert_eq!(offset_of!(Resource, state), RESOURCE_STATE);
        assert_eq!(offset_of!(Resource, description) + 4, RESOURCE_FORMAT);
        assert_eq!(offset_of!(Resource, description) + 8, RESOURCE_WIDTH);
        assert_eq!(offset_of!(Resource, description) + 12, RESOURCE_HEIGHT);
        assert_eq!(offset_of!(Resource, description) + 28, RESOURCE_USAGE);
        assert_eq!(offset_of!(DispatchUpscale, command_list), COMMAND_LIST);
        for (i, &(at, write, _)) in UPSCALE.iter().enumerate() {
            assert_eq!(offset_of!(DispatchUpscale, resources) + i * size_of::<Resource>(), at);
            // Only the output is written.
            assert_eq!(write, i == 6);
        }
        for (i, &(at, write, _)) in REACTIVE_MASK.iter().enumerate() {
            assert_eq!(offset_of!(DispatchUpscale, resources) + i * size_of::<Resource>(), at);
            assert_eq!(write, i == 2);
        }
        // The rest, where the PC DLL's dispatch reads them (read).
        assert_eq!(offset_of!(DispatchUpscale, jitter), JITTER);
        assert_eq!(offset_of!(DispatchUpscale, motion_vector_scale), MOTION_VECTOR_SCALE);
        assert_eq!(offset_of!(DispatchUpscale, render_size), RENDER_SIZE);
        assert_eq!(offset_of!(DispatchUpscale, upscale_size), UPSCALE_SIZE);
        assert_eq!(offset_of!(DispatchUpscale, sharpening), SHARPENING);
        assert_eq!(offset_of!(DispatchUpscale, sharpness), SHARPNESS);
        assert_eq!(offset_of!(DispatchUpscale, frame_time), FRAME_TIME);
        assert_eq!(offset_of!(DispatchUpscale, pre_exposure), PRE_EXPOSURE);
        assert_eq!(offset_of!(DispatchUpscale, reset), RESET);
        assert_eq!(offset_of!(DispatchUpscale, camera), CAMERA);
        assert_eq!(offset_of!(DispatchUpscale, flags), FLAGS);
        assert_eq!(size_of::<DispatchUpscale>(), 432);
        assert_eq!(offset_of!(CreateUpscale, message), CREATE_MESSAGE);
    }

    #[test]
    fn an_upscale_is_read_as_the_title_passes_it() {
        let mut desc = DispatchUpscale {
            ty: DISPATCH_UPSCALE,
            next: std::ptr::null_mut(),
            command_list: std::ptr::null_mut(),
            resources: std::array::from_fn(|_| Resource { resource: std::ptr::null_mut(), description: [0; 8], state: 0 }),
            jitter: [0.25, -0.5],
            motion_vector_scale: [1280.0, 720.0],
            render_size: [1280, 720],
            upscale_size: [1920, 1080],
            sharpening: false,
            sharpness: 0.0,
            frame_time: 33.3,
            pre_exposure: 1.0,
            reset: true,
            camera: [0.1, 10000.0, 1.0, 1.0],
            flags: 0,
        };
        desc.resources[0].resource = 0x1000 as *mut c_void;
        desc.resources[6].resource = 0x6000 as *mut c_void;
        let at = (&raw mut desc).cast::<u8>();
        let upscale = Upscale::of(at);
        assert_eq!((upscale.colour as usize, upscale.output as usize), (0x1000, 0x6000));
        assert_eq!((upscale.render, upscale.upscale), ([1280, 720], [1920, 1080]));
        let fields = upscale_fields(at);
        assert!(fields.starts_with("render 1280x720, upscale 1920x1080, jitter (0.25, -0.5)"), "{fields}");
        assert!(fields.contains("reset 1"), "{fields}");
    }

    #[test]
    fn surface_formats_are_the_pc_dll_s() {
        // The three the header names, and the stand-ins of R9G9B9E5.
        assert_eq!(dxgi_format(4), Some(10));
        assert_eq!(dxgi_format(18), Some(34));
        assert_eq!(dxgi_format(28), Some(41));
        assert_eq!(dxgi_format(29), Some(67));
        assert_eq!(surface_format(26), Some(16));
        assert_eq!(surface_format(10), Some(4));
        assert_eq!(dxgi_format(0), None);
        assert_eq!(dxgi_format(40), None);
        assert_eq!(surface_format(0), None);
        assert_eq!(surface_format(45), None);
        for (surface, &dxgi) in SURFACE_FORMATS.iter().enumerate().skip(1) {
            assert_eq!(surface_format(dxgi), Some(surface as u32), "DXGI {dxgi}");
        }
    }

    #[test]
    fn the_pc_dll_is_told_the_host_copy_s_format_where_the_title_s_is_another() {
        // The title's R9G9B9E5 output on its stand-ins.
        assert_eq!(told_format(29, 26), Some(16));
        assert_eq!(told_format(29, 10), Some(4));
        // Its colour as it is.
        assert_eq!(told_format(4, 10), None);
        // Depth, described as R32_FLOAT, keeps that on the depth host copies
        // of 32 and 24 bits (run cf-fsr2: the title's is
        // R32G8X24_TYPELESS): the DLL views those in their depth plane's
        // format whatever it is told.
        for host in [19, 20, 21, 39, 40, 44, 45, 55] {
            assert_eq!(told_format(28, host), None, "host format {host}");
        }
        // An sRGB texture written through unordered access, typeless on the
        // host, keeps the title's.
        assert_eq!(told_format(12, 27), None);
        // A format the PC DLL does not know.
        assert_eq!(told_format(0, 10), Some(4));
        assert_eq!(told_format(77, 34), Some(18));
    }
}
