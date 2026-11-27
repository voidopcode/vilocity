//! (Neptune-VE) Host objects for PC libraries the title's calls are
//! redirected to: the FidelityFX upscaler of the PC (`VILOCITY_FSR_DLL`,
//! `vilocity-hle`'s `fidelityfx`). The title holds our console objects; a PC
//! library needs the host device, the host command list the title's list
//! records onto, and the host textures behind the title's.
//!
//! Rebuilt on this tree's lists and textures. The library records onto the
//! host list behind the title's list at the call ([`record`]), so its
//! commands run after the title's commands recorded before the call and
//! before those recorded after it, as the console library's do; with
//! `VILOCITY_THIN=1` once what the title wrote down so far is translated
//! (`crate::list::with_host`). Each texture is made ready as a shader's use
//! of it is (`crate::draw`'s `readable`): one the title fills is uploaded
//! first, a depth texture first gets what was written to its colour twin,
//! and each is moved by the list's own state tracking to the state the
//! library is told it is in ([`READ`], [`WRITE`]). A FidelityFX library
//! puts what it is given back into the state it was told of when its
//! dispatch ends, with an unordered access barrier on what it was told is in
//! that state (read: the PC upscaler 4.1.0's backend, the resources'
//! unregistering at the end of each dispatch), so the tracking still holds
//! after it. A depth texture is read with both planes in [`READ`] (the
//! list's barriers name every subresource). The texture it
//! writes is any shader's write: copied back into title memory when the
//! list closes if it is of a kind that is (`crate::texback`), given its
//! levels' memory first if it is made level by level (`crate::tiles`).
//! What the library binds (heaps, root signature, pipeline) is set again
//! for the title's next draw or dispatch (`crate::draw::State::after_external`).
//! For a frame the library does not do, a pass of ours can scale its input
//! into its output in its place ([`Pass::scale`]).
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_void;

use windows::Win32::Graphics::Direct3D12::{D3D12_RESOURCE_BARRIER, D3D12_RESOURCE_BARRIER_TYPE_UAV};
use windows::core::Interface;

use crate::object::This;

/// The host state a texture a library reads is in for its work: read by
/// compute shaders (`D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE`).
pub const READ: u32 = 0x40;
/// The host state a texture a library writes is in for its work: unordered
/// access (`D3D12_RESOURCE_STATE_UNORDERED_ACCESS`).
pub const WRITE: u32 = 0x8;

// Resource flags of a host texture (`D3D12_RESOURCE_FLAGS`).
const RENDER_TARGET: u32 = 0x1;
const DEPTH_STENCIL: u32 = 0x2;
const UNORDERED_ACCESS: u32 = 0x4;
const DENY_SHADER_RESOURCE: u32 = 0x8;

/// The host `ID3D12Device` (no reference taken), if there is a host GPU.
pub fn host_device() -> Option<*mut c_void> {
    crate::host::host().map(|h| h.device.as_raw())
}

/// Whether `object` is the device the title created.
pub fn is_title_device(object: *const c_void) -> bool {
    std::ptr::eq(object, (&raw const crate::device::DEVICE).cast())
}

/// Whether `object` is a command list of ours.
pub fn is_title_list(object: *const c_void) -> bool {
    crate::list::is_list(object.cast())
}

/// Whether `object` is a live title texture.
pub fn is_title_texture(object: *const c_void) -> bool {
    crate::resource::is_texture(object.cast())
}

/// A title texture's host copy as a library would be handed it, and what
/// the title made it as, for the description that goes with it.
#[derive(Clone, Copy, Debug)]
pub struct Texture {
    /// The host `ID3D12Resource` (no reference taken).
    pub resource: *mut c_void,
    /// The host copy's DXGI format, size, levels and resource flags.
    pub format: u32,
    pub width: u64,
    pub height: u32,
    pub levels: u16,
    pub flags: u32,
    /// The title's own format, width and height for it; `None` if it is
    /// not one 2D image.
    pub title: Option<(u32, u64, u32)>,
    /// The host copy's format stands in for the title's, which the host
    /// does not render to (`crate::resource::target_format`): the title's
    /// description of it names another format. The title's own views of it
    /// name the stand-in too, so what a library writes in the host copy's
    /// format is what the title's later reads find.
    pub stand_in: bool,
}

impl Texture {
    /// Whether the host texture allows unordered access: a library makes
    /// such a view of what it writes.
    pub fn unordered(&self) -> bool {
        self.flags & UNORDERED_ACCESS != 0
    }

    /// Whether shaders may read the host texture: a library makes shader
    /// resource views of what it reads.
    pub fn readable(&self) -> bool {
        self.flags & DENY_SHADER_RESOURCE == 0
    }
}

/// Whether DXGI formats `a` and `b` are of one family
/// (`crate::resource::family`): a view in one may be made of a texture in
/// the other.
pub fn same_family(a: u32, b: u32) -> bool {
    crate::resource::family(a) == crate::resource::family(b)
}

/// One title list's host list, open for a PC library to record onto.
pub struct Pass<'a> {
    list: &'a crate::host::HostList,
    // (Vilocity: DLSS through Streamline is louketa/foxhound's, lucas-3080 56dba41, GPL-3.0-or-later, as this.)
    /// The library's work may have set more than a compute library sets
    /// ([`Pass::all_again`]).
    again: bool,
}

impl Pass<'_> {
    // (Vilocity: DLSS through Streamline is louketa/foxhound's, lucas-3080 56dba41, GPL-3.0-or-later, as this.)
    /// The host list itself, for work of ours in this crate (`crate::streamline`).
    pub(crate) fn host_list(&self) -> &crate::host::HostList {
        self.list
    }

    /// After this pass everything the title set on its list is set again
    /// (`crate::draw::State::replay`), not only what a compute library sets
    /// (`crate::draw::State::after_external`): as foxhound does after DLSS
    /// recorded an upscaling.
    pub(crate) fn all_again(&mut self) {
        self.again = true;
    }

    /// The host `ID3D12GraphicsCommandList` (no reference taken).
    pub fn command_list(&self) -> *mut c_void {
        self.list.list.as_raw()
    }

    /// Title texture `texture` as a library would be handed it, without
    /// recording anything: a dispatch that cannot be done leaves the list as
    /// it was. Whether its views can be made (`Texture::unordered`,
    /// `Texture::readable`; a view the host copy does not allow is invalid
    /// and may remove the device) is the caller's to say. `Err`: there is
    /// nothing to hand over.
    pub fn look(&self, texture: *const c_void) -> Result<Texture, &'static str> {
        let title: This = texture.cast();
        if !crate::resource::is_texture(title) {
            return Err("not a live texture of the title");
        }
        let Some(copy) = crate::resource::host_copy(title) else {
            return Err("the host has no copy of it");
        };
        // SAFETY: a live resource.
        let d = unsafe { copy.texture.GetDesc() };
        Ok(Texture {
            resource: copy.texture.as_raw(),
            format: d.Format.0 as u32,
            width: d.Width,
            height: d.Height,
            levels: d.MipLevels,
            flags: copy.flags(),
            title: crate::resource::texture_kind(title).map(|(width, height, format)| (format, width, height)),
            stand_in: crate::resource::stands_in(title),
        })
    }

    /// Makes title texture `texture` ready for the library on this list, as
    /// a shader's read (or, with `write`, its unordered access write) is
    /// prepared, and moves it to [`READ`] (or [`WRITE`]); the list holds
    /// its host copy from here until it has run. The host resource; `None`
    /// if the texture has no host copy (any more).
    pub fn ready(&mut self, texture: *const c_void, write: bool) -> Option<*mut c_void> {
        let title: This = texture.cast();
        let copy = crate::resource::host_copy(title)?;
        if write {
            // Some textures shaders write are copied back into title memory
            // when the list closes (`crate::texback`).
            self.list.texture_written(title);
            // A texture made level by level gets its levels' memory ahead
            // of this list (`crate::tiles`); before the state of the write.
            crate::texture::card_writes(self.list, title, &copy.texture, None);
            crate::list::ensure_state(self.list, title, WRITE);
            #[cfg(feature = "diagnostics")]
            crate::capture::note_target(title);
        } else {
            // A view the title placed in the memory of a texture the host
            // writes, in another format: that texture's texels, copied on
            // the host (title memory does not have them).
            let aliased = crate::resource::alias_of(title)
                .is_some_and(|source| crate::texture::alias(self.list, title, source));
            // What the title fills itself is uploaded first
            // (`crate::texture::title_written`: no target, depth or
            // unordered access).
            if !aliased && copy.flags() & (RENDER_TARGET | DEPTH_STENCIL | UNORDERED_ACCESS) == 0 {
                crate::texture::upload_from(self.list, title, 0);
            }
            // A depth texture shaders also write: what they wrote, first.
            self.list.twin_use(title, false, false);
            crate::list::ensure_state(self.list, title, READ);
        }
        Some(copy.texture.as_raw())
    }

    /// For a frame the library did not do: records a pass of ours that fills
    /// the rectangle `to_size` at the top left of title texture `to` with
    /// the rectangle `from_size` at the top left of title texture `from`,
    /// scaled with linear filtering (a size of 0 is the texture's own). For
    /// an upscaler that is its picture without the detail it adds, where the
    /// title would otherwise go on with whatever `to` held. Both textures
    /// are made ready as for the library (`from` read, `to` written through
    /// unordered access).
    pub fn scale(&mut self, from: *const c_void, from_size: [u32; 2], to: *const c_void, to_size: [u32; 2]) -> Result<(), &'static str> {
        let (source, target) = (self.look(from)?, self.look(to)?);
        if !source.readable() {
            return Err("its input denies shader reads");
        }
        if !target.unordered() {
            return Err("its output allows no unordered access");
        }
        let read = crate::resource::view_format(from.cast()).unwrap_or(source.format);
        let write = unordered_format(to.cast(), target.format);
        let texture = [source.width.min(u64::from(u32::MAX)) as u32, source.height];
        let whole = [target.width.min(u64::from(u32::MAX)) as u32, target.height];
        let size = |asked: [u32; 2], whole: [u32; 2]| {
            [0, 1].map(|i| if asked[i] == 0 { whole[i] } else { asked[i].min(whole[i]) })
        };
        let (from_size, to_size) = (size(from_size, texture), size(to_size, whole));
        if from_size.contains(&0) || to_size.contains(&0) {
            return Err("an empty texture");
        }
        let scaler = scaler().ok_or("the pass of ours could not be made")?;
        let (Some(_), Some(_)) = (self.ready(from, false), self.ready(to, true)) else {
            return Err("a texture lost its host copy");
        };
        let (Some(source), Some(target)) = (crate::resource::host_copy(from.cast()), crate::resource::host_copy(to.cast())) else {
            return Err("a texture lost its host copy");
        };
        // Where an output texel's centre falls in the input, as a fraction
        // of its texture: the same place in the rectangle read.
        let scale = [0, 1].map(|i| from_size[i] as f32 / (to_size[i] as f32 * texture[i] as f32));
        scaler.record(self.list, (&source.texture, read), (&target.texture, write), scale, to_size);
        Ok(())
    }
}

/// The format of an unordered access view of title texture `title`, whose
/// host copy is in `format`: a typeless host copy of an sRGB texture is
/// written as its linear twin (the host has no sRGB unordered access).
fn unordered_format(title: This, format: u32) -> u32 {
    match crate::resource::view_format(title) {
        Some(own) => crate::resource::SRGB.iter().find(|&&(srgb, _)| srgb == own).map_or(own, |&(_, linear)| linear),
        None => format,
    }
}

/// The pass of [`Pass::scale`]: a compute program of ours and a ring of
/// view pairs for it.
struct Scaler {
    root: windows::Win32::Graphics::Direct3D12::ID3D12RootSignature,
    pso: windows::Win32::Graphics::Direct3D12::ID3D12PipelineState,
    views: windows::Win32::Graphics::Direct3D12::ID3D12DescriptorHeap,
    step: u32,
    next: std::sync::atomic::AtomicU32,
}

// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Scaler {}
// SAFETY: as above.
unsafe impl Sync for Scaler {}

/// View pairs in the ring: a pair is written when its pass is recorded and
/// read when the list runs, so the ring is many frames of passes long.
const SCALES: u32 = 256;

const SCALE_SOURCE: &str = r#"
#define RS "DescriptorTable(SRV(t0), UAV(u0)), RootConstants(num32BitConstants=4, b0), StaticSampler(s0, filter=FILTER_MIN_MAG_MIP_LINEAR, addressU=TEXTURE_ADDRESS_CLAMP, addressV=TEXTURE_ADDRESS_CLAMP)"
Texture2D<float4> from_ : register(t0);
RWTexture2D<float4> to_ : register(u0);
SamplerState smooth_ : register(s0);
cbuffer Sizes : register(b0) { float2 scale; uint2 size; };
[RootSignature(RS)]
[numthreads(8, 8, 1)]
void main(uint3 at : SV_DispatchThreadID) {
    if (at.x >= size.x || at.y >= size.y) return;
    to_[at.xy] = from_.SampleLevel(smooth_, (float2(at.xy) + 0.5) * scale, 0);
}
"#;

/// The pass of ours, made on first use; `None` if it cannot be (said once).
fn scaler() -> Option<&'static Scaler> {
    static SCALER: std::sync::OnceLock<Option<Scaler>> = std::sync::OnceLock::new();
    SCALER
        .get_or_init(|| {
            Scaler::new()
                .inspect_err(|e| tracing::warn!(error = %e, "no pass of ours to scale a library's input into its output"))
                .ok()
        })
        .as_ref()
}

impl Scaler {
    fn new() -> windows::core::Result<Scaler> {
        use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
        use windows::Win32::Graphics::Direct3D::ID3DBlob;
        use windows::Win32::Graphics::Direct3D12::*;
        use windows::core::s;
        let host = crate::host::host().ok_or_else(windows::core::Error::empty)?;
        let mut code: Option<ID3DBlob> = None;
        let mut errors: Option<ID3DBlob> = None;
        // SAFETY: the source and names live through the call; out pointers ours.
        let compiled = unsafe {
            D3DCompile(
                SCALE_SOURCE.as_ptr().cast(),
                SCALE_SOURCE.len(),
                s!("scale"),
                None,
                None,
                s!("main"),
                s!("cs_5_1"),
                0,
                0,
                &mut code,
                Some(&mut errors),
            )
        };
        if let Err(e) = compiled {
            if let Some(errors) = errors {
                // SAFETY: the blob's own bytes.
                let text = unsafe {
                    std::slice::from_raw_parts(errors.GetBufferPointer().cast::<u8>(), errors.GetBufferSize())
                };
                tracing::warn!("the scaling pass of ours: {}", String::from_utf8_lossy(text));
            }
            return Err(e);
        }
        let code = code.ok_or_else(windows::core::Error::empty)?;
        // SAFETY: the blob's own bytes, alive through the calls; the root
        // signature is inside the program.
        unsafe {
            let bytes = std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize());
            let root: ID3D12RootSignature = host.device.CreateRootSignature(0, bytes)?;
            let pso = host.device.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                pRootSignature: std::mem::transmute_copy(&root),
                CS: D3D12_SHADER_BYTECODE { pShaderBytecode: code.GetBufferPointer(), BytecodeLength: code.GetBufferSize() },
                ..Default::default()
            })?;
            let views = host.device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: 2 * SCALES,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                NodeMask: 0,
            })?;
            let step = host.device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
            Ok(Scaler { root, pso, views, step, next: std::sync::atomic::AtomicU32::new(0) })
        }
    }

    /// Records the pass onto `list`: `from` read in its format, `to` written
    /// in its; `scale` takes an output texel to its place in the input.
    fn record(
        &self,
        list: &crate::host::HostList,
        (from, read): (&windows::Win32::Graphics::Direct3D12::ID3D12Resource, u32),
        (to, write): (&windows::Win32::Graphics::Direct3D12::ID3D12Resource, u32),
        scale: [f32; 2],
        size: [u32; 2],
    ) {
        use windows::Win32::Graphics::Direct3D12::*;
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
        let pair = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % SCALES;
        let at = (pair * 2 * self.step) as usize;
        // SAFETY: an open list; views made in a pair no list in flight still
        // reads (the ring is many frames long); live textures.
        unsafe {
            let cpu = self.views.GetCPUDescriptorHandleForHeapStart();
            let gpu = self.views.GetGPUDescriptorHandleForHeapStart();
            let srv = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT(read as i32),
                ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_SRV { MipLevels: 1, ..Default::default() } },
            };
            let host = crate::host::host().map(|h| &h.device);
            let Some(device) = host else { return };
            device.CreateShaderResourceView(from, Some(&srv), D3D12_CPU_DESCRIPTOR_HANDLE { ptr: cpu.ptr + at });
            let uav = D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT(write as i32),
                ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2D,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_UAV::default() },
            };
            device.CreateUnorderedAccessView(
                to,
                None,
                Some(&uav),
                D3D12_CPU_DESCRIPTOR_HANDLE { ptr: cpu.ptr + at + self.step as usize },
            );
            let constants = [scale[0].to_bits(), scale[1].to_bits(), size[0], size[1]];
            let l = &list.list;
            l.SetDescriptorHeaps(&[Some(self.views.clone())]);
            l.SetComputeRootSignature(&self.root);
            l.SetPipelineState(&self.pso);
            l.SetComputeRootDescriptorTable(0, D3D12_GPU_DESCRIPTOR_HANDLE { ptr: gpu.ptr + at as u64 });
            l.SetComputeRoot32BitConstants(1, 4, constants.as_ptr().cast(), 0);
            l.Dispatch(size[0].div_ceil(8), size[1].div_ceil(8), 1);
        }
    }
}

impl Pass<'_> {
    /// (Vilocity) For a program of the title no card here can run: title
    /// texture `to` (its first level, every slice or depth) set to `value`
    /// by a pass of ours, made ready as for an unordered write.
    pub(crate) fn fill(&mut self, to: *const c_void, value: [f32; 4]) -> Result<(), &'static str> {
        let target = self.look(to)?;
        if !target.unordered() {
            return Err("its host copy allows no unordered access");
        }
        let filler = filler().ok_or("the pass of ours could not be made")?;
        self.ready(to, true).ok_or("a texture lost its host copy")?;
        let copy = crate::resource::host_copy(to.cast()).ok_or("a texture lost its host copy")?;
        // SAFETY: a live resource.
        let desc = unsafe { copy.texture.GetDesc() };
        filler.record(self.list, &copy.texture, unordered_format(to.cast(), desc.Format.0 as u32), &desc, value)
    }
}

/// (Vilocity) `f` with a pass on host list `list`, which the caller is
/// recording already (a draw state's own dispatch, `crate::draw`): the
/// caller sets its draw state again afterwards (`State::after_external`).
pub(crate) fn record_here(list: &crate::host::HostList, f: impl FnOnce(&mut Pass)) {
    uav_barrier(list);
    let mut pass = Pass { list, again: false };
    f(&mut pass);
    uav_barrier(list);
    list.changed_something();
}

/// The pass of [`Pass::fill`]: one program for 2D textures (as arrays), one
/// for volumes, and a ring of views for them.
struct Filler {
    root: windows::Win32::Graphics::Direct3D12::ID3D12RootSignature,
    flat: windows::Win32::Graphics::Direct3D12::ID3D12PipelineState,
    volume: windows::Win32::Graphics::Direct3D12::ID3D12PipelineState,
    views: windows::Win32::Graphics::Direct3D12::ID3D12DescriptorHeap,
    step: u32,
    next: std::sync::atomic::AtomicU32,
}

// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Filler {}
// SAFETY: as above.
unsafe impl Sync for Filler {}

const FILLS: u32 = 256;

const FILL_SOURCE: &str = r#"
#define RS "DescriptorTable(UAV(u0)), RootConstants(num32BitConstants=8, b0)"
#ifdef VOLUME
RWTexture3D<float4> to_ : register(u0);
#else
RWTexture2DArray<float4> to_ : register(u0);
#endif
cbuffer Fill : register(b0) { float4 value; uint3 size; uint unused; };
[RootSignature(RS)]
[numthreads(4, 4, 4)]
void main(uint3 at : SV_DispatchThreadID) {
    if (any(at >= size)) return;
    to_[at] = value;
}
"#;

fn filler() -> Option<&'static Filler> {
    static FILLER: std::sync::OnceLock<Option<Filler>> = std::sync::OnceLock::new();
    FILLER
        .get_or_init(|| {
            Filler::new().inspect_err(|e| tracing::warn!(error = %e, "no pass of ours to fill a texture")).ok()
        })
        .as_ref()
}

impl Filler {
    fn new() -> windows::core::Result<Filler> {
        use windows::Win32::Graphics::Direct3D12::*;
        use windows::core::s;
        let host = crate::host::host().ok_or_else(windows::core::Error::empty)?;
        let flat = crate::host::compile(FILL_SOURCE, s!("main"), s!("cs_5_1"))?;
        let volume = crate::host::compile(&format!("#define VOLUME\n{FILL_SOURCE}"), s!("main"), s!("cs_5_1"))?;
        // SAFETY: compiled programs with their root signature inside; plain creations.
        unsafe {
            let root: ID3D12RootSignature = host.device.CreateRootSignature(0, &flat)?;
            let pso = |code: &[u8]| {
                host.device.CreateComputePipelineState::<ID3D12PipelineState>(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&root),
                    CS: D3D12_SHADER_BYTECODE { pShaderBytecode: code.as_ptr().cast(), BytecodeLength: code.len() },
                    ..Default::default()
                })
            };
            let (flat, volume) = (pso(&flat)?, pso(&volume)?);
            let views = host.device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: FILLS,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                NodeMask: 0,
            })?;
            let step = host.device.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
            Ok(Filler { root, flat, volume, views, step, next: std::sync::atomic::AtomicU32::new(0) })
        }
    }

    /// Records the fill of `to` (in format `write`) onto `list`.
    fn record(
        &self,
        list: &crate::host::HostList,
        to: &windows::Win32::Graphics::Direct3D12::ID3D12Resource,
        write: u32,
        desc: &windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_DESC,
        value: [f32; 4],
    ) -> Result<(), &'static str> {
        use windows::Win32::Graphics::Direct3D12::*;
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;
        let volume = desc.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D;
        if !volume && desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D {
            return Err("not a 2D texture or a volume");
        }
        let size = [desc.Width.min(u64::from(u32::MAX)) as u32, desc.Height, u32::from(desc.DepthOrArraySize)];
        let uav = if volume {
            D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT(write as i32),
                ViewDimension: D3D12_UAV_DIMENSION_TEXTURE3D,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 { Texture3D: D3D12_TEX3D_UAV { MipSlice: 0, FirstWSlice: 0, WSize: size[2] } },
            }
        } else {
            D3D12_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT(write as i32),
                ViewDimension: D3D12_UAV_DIMENSION_TEXTURE2DARRAY,
                Anonymous: D3D12_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Texture2DArray: D3D12_TEX2D_ARRAY_UAV { MipSlice: 0, FirstArraySlice: 0, ArraySize: size[2], PlaneSlice: 0 },
                },
            }
        };
        let device = &crate::host::host().ok_or("no host device")?.device;
        let at = (self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % FILLS * self.step) as usize;
        let mut constants = [0u32; 8];
        for (to, v) in constants.iter_mut().zip(value) {
            *to = v.to_bits();
        }
        constants[4..7].copy_from_slice(&size);
        // SAFETY: an open list; a view made in a slot no list in flight still
        // reads (the ring is many frames long); a live texture.
        unsafe {
            let cpu = self.views.GetCPUDescriptorHandleForHeapStart();
            let gpu = self.views.GetGPUDescriptorHandleForHeapStart();
            device.CreateUnorderedAccessView(to, None, Some(&uav), D3D12_CPU_DESCRIPTOR_HANDLE { ptr: cpu.ptr + at });
            let l = &list.list;
            l.SetDescriptorHeaps(&[Some(self.views.clone())]);
            l.SetComputeRootSignature(&self.root);
            l.SetPipelineState(if volume { &self.volume } else { &self.flat });
            l.SetComputeRootDescriptorTable(0, D3D12_GPU_DESCRIPTOR_HANDLE { ptr: gpu.ptr + at as u64 });
            l.SetComputeRoot32BitConstants(1, 8, constants.as_ptr().cast(), 0);
            l.Dispatch(size[0].div_ceil(4), size[1].div_ceil(4), size[2].div_ceil(4));
        }
        Ok(())
    }
}

/// Runs `f` with title list `list`'s host list, at this point of the title's
/// commands; afterwards the title's heaps, root signatures and pipeline are
/// set again for its next draw or dispatch (the library set its own).
/// `false` if `list` is not a list of ours with a host list.
pub fn record(list: *const c_void, f: impl FnOnce(&mut Pass)) -> bool {
    crate::list::with_host(list.cast(), |host, state| {
        #[cfg(feature = "diagnostics")]
        host.note_op(|| "work of a PC library (crate::external)".to_owned());
        // What shaders wrote before is done before the library's work, and
        // what the library writes before the title's next work: a texture
        // left in the unordered access state in between gets no transition
        // that would order the two.
        uav_barrier(host);
        let mut pass = Pass { list: host, again: false };
        f(&mut pass);
        uav_barrier(host);
        // Textures changed: draws after this bring theirs up to date again.
        host.changed_something();
        // (Vilocity: DLSS through Streamline is louketa/foxhound's, lucas-3080 56dba41, GPL-3.0-or-later, as this.)
        // After DLSS all of it is set again (`Pass::all_again`).
        if pass.again {
            state.replay(host);
        } else {
            state.after_external();
        }
    })
}

/// An unordered access barrier on every resource.
fn uav_barrier(list: &crate::host::HostList) {
    let barrier = D3D12_RESOURCE_BARRIER { Type: D3D12_RESOURCE_BARRIER_TYPE_UAV, ..Default::default() };
    // SAFETY: an open list; the barrier names no resource.
    unsafe { list.list.ResourceBarrier(&[barrier]) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_objects_are_taken_for_the_title_s() {
        let word = 0u64;
        let local = (&raw const word).cast::<c_void>();
        assert!(!is_title_list(std::ptr::null()));
        assert!(!is_title_list(local));
        assert!(!is_title_texture(std::ptr::null()));
        assert!(!is_title_texture(local));
        assert!(!is_title_device(local));
        assert!(is_title_device((&raw const crate::device::DEVICE).cast()));
    }

    #[test]
    fn a_buffer_is_not_a_texture() {
        let buffer = crate::resource::test_buffer(256);
        assert!(crate::resource::is_resource(buffer));
        assert!(!is_title_texture(buffer.cast()));
    }
}
