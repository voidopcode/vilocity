//! Changed bytes of a copy's CPU side put into its card side in one pass.
//!
//! What the title writes into memory that shaders in flight may also write
//! goes up byte-exact: only the bytes the title changed, so that what the
//! shaders write beside them stays (`Shadow::merge_under_writes`). A copy of
//! its own for each changed run did that, but the title changes 60 to 70
//! thousand runs of a few bytes a frame in one buffer (run
//! gta6-20261125-154843): more copies than a list should hold, so the
//! flights were settled instead, about 100 ms a frame of the card standing
//! idle. Here one dispatch writes every run: whole dwords with a store, the
//! dwords a run covers in part with an atomic AND that clears its bytes and
//! an atomic OR that sets them, so a neighbour's bytes in the same dword
//! are never written.
//!
//! The runs lie at the same offsets on both sides (the CPU side is a copy
//! of the card side's memory, byte for byte).

use std::sync::OnceLock;
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::s;

/// A run longer than this is split, so that the threads of a dispatch do
/// about the same work.
const CHUNK: u64 = 256;
/// Fewer runs than this go up as copies, as before.
pub(crate) const FEWEST: usize = 32;

const SOURCE: &str = r#"
#define RS "RootConstants(num32BitConstants=1, b0), SRV(t0), SRV(t1), UAV(u0)"
cbuffer Count : register(b0) { uint count; };
// The CPU side, the runs (offset, length), the card side.
ByteAddressBuffer src : register(t0);
ByteAddressBuffer runs : register(t1);
RWByteAddressBuffer dst : register(u0);
[RootSignature(RS)]
[numthreads(64, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= count) return;
    uint2 r = runs.Load2(id.x * 8);
    uint at = r.x;
    uint end = r.x + r.y;
    for (uint w = at & ~3u; w < end; w += 4) {
        uint lo = max(at, w);
        uint hi = min(end, w + 4);
        uint v = src.Load(w);
        if (hi - lo == 4) {
            dst.Store(w, v);
        } else {
            uint mask = ((1u << ((hi - lo) * 8)) - 1u) << ((lo - w) * 8);
            uint was;
            dst.InterlockedAnd(w, ~mask, was);
            dst.InterlockedOr(w, v & mask, was);
        }
    }
}
"#;

/// The pass: its root signature and pipeline.
pub(crate) struct Scatter {
    root: ID3D12RootSignature,
    pso: ID3D12PipelineState,
}
// SAFETY: D3D12 objects are free-threaded.
unsafe impl Send for Scatter {}
// SAFETY: as above.
unsafe impl Sync for Scatter {}

fn make(device: &ID3D12Device) -> windows::core::Result<Scatter> {
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: the source and names live through the call; out pointers ours.
    let compiled = unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            s!("scatter"),
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
    let bytes = |b: &ID3DBlob| {
        // SAFETY: the blob's own bytes.
        unsafe { std::slice::from_raw_parts(b.GetBufferPointer().cast::<u8>(), b.GetBufferSize()) }.to_vec()
    };
    if let Err(e) = compiled {
        if let Some(errors) = errors {
            tracing::warn!("scatter pass: {}", String::from_utf8_lossy(&bytes(&errors)));
        }
        return Err(e);
    }
    let code = bytes(&code.ok_or_else(windows::core::Error::empty)?);
    // SAFETY: the root signature is inside the shader; plain creations.
    unsafe {
        let root: ID3D12RootSignature = device.CreateRootSignature(0, &code)?;
        let pso = device.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
            pRootSignature: std::mem::transmute_copy(&root),
            CS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: code.as_ptr().cast(),
                BytecodeLength: code.len(),
            },
            ..Default::default()
        })?;
        Ok(Scatter { root, pso })
    }
}

/// The pass on the host's device; `None` if it cannot be made (then the
/// runs go up as copies, and a merge with too many of them settles).
/// `VILOCITY_SCATTER=0`: not used.
pub(crate) fn get() -> Option<&'static Scatter> {
    static PASS: OnceLock<Option<Scatter>> = OnceLock::new();
    PASS.get_or_init(|| {
        if std::env::var_os("VILOCITY_SCATTER").is_some_and(|v| v == "0") {
            return None;
        }
        let host = crate::host::host()?;
        make(&host.device).inspect_err(|e| tracing::warn!(error = %e, "no scatter pass")).ok()
    })
    .as_ref()
}

/// The table of `ranges` (offset, end) for [`Scatter::record`]: each range
/// as runs of at most [`CHUNK`] bytes, two dwords each. `None` if an
/// offset does not fit 32 bits.
pub(crate) fn table(ranges: &[(usize, usize)]) -> Option<Vec<u32>> {
    let mut out = Vec::with_capacity(ranges.len() * 2);
    for &(a, b) in ranges {
        let (mut at, end) = (a as u64, b as u64);
        if end > u64::from(u32::MAX) {
            return None;
        }
        while at < end {
            let n = (end - at).min(CHUNK);
            out.extend_from_slice(&[at as u32, n as u32]);
            at += n;
        }
    }
    Some(out)
}

/// A run table in memory the card reads ([`upload`]): given back to a pool
/// when dropped, so that the 30 or so tables of a frame do not each make a
/// buffer (fresh memory: kernel time to zero and map it).
pub(crate) struct Table {
    pub(crate) buffer: ID3D12Resource,
    data: usize,
    size: u64,
}

/// Tables not in use, at most [`POOLED`].
static POOL: std::sync::Mutex<Vec<(ID3D12Resource, usize, u64)>> = std::sync::Mutex::new(Vec::new());
const POOLED: usize = 64;

// SAFETY: D3D12 objects are free-threaded; the mapping is only written by
// the one holder of the table.
unsafe impl Send for Table {}
// SAFETY: as above.
unsafe impl Sync for Table {}

impl Drop for Table {
    fn drop(&mut self) {
        let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
        if pool.len() < POOLED {
            pool.push((self.buffer.clone(), self.data, self.size));
        }
    }
}

/// `words` (a [`table`]) in memory the card reads: a pooled buffer big
/// enough, else a new one of the next power of two (64 KiB at least).
pub(crate) fn upload(words: &[u32]) -> Option<Table> {
    let bytes = (words.len() * 4) as u64;
    let kept = {
        let mut pool = POOL.lock().unwrap_or_else(|e| e.into_inner());
        let fit = pool
            .iter()
            .enumerate()
            .filter(|(_, t)| t.2 >= bytes)
            .min_by_key(|(_, t)| t.2)
            .map(|(i, _)| i);
        fit.map(|i| pool.swap_remove(i))
    };
    let (buffer, data, size) = match kept {
        Some(t) => t,
        None => {
            let size = bytes.max(1 << 16).next_power_of_two();
            let (buffer, data) = crate::host::host()?.shared_buffer(size)?;
            crate::host::name(&buffer, || "scatter pass: changed runs".into());
            (buffer, data as usize, size)
        }
    };
    // SAFETY: a mapping of `size` bytes, `bytes` of them written; nothing
    // else holds this buffer now.
    unsafe { std::ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>(), data as *mut u8, bytes as usize) };
    Some(Table { buffer, data, size })
}

impl Scatter {
    /// Records into `list` the runs of `table` (made by [`table`], in
    /// `runs`, a buffer the card can read) from `src` into `dst`. `dst`
    /// must be in the unordered-access state; a UAV barrier follows.
    ///
    /// # Safety
    /// An open list; live buffers that hold every run; `runs` holds
    /// `count` runs and stays alive until the list has run.
    pub(crate) unsafe fn record(
        &self,
        list: &ID3D12GraphicsCommandList,
        src: &ID3D12Resource,
        dst: &ID3D12Resource,
        runs: &ID3D12Resource,
        count: u32,
    ) {
        if count == 0 {
            return;
        }
        // SAFETY: as the caller promises.
        unsafe {
            list.SetComputeRootSignature(&self.root);
            list.SetPipelineState(&self.pso);
            list.SetComputeRoot32BitConstants(0, 1, (&raw const count).cast(), 0);
            list.SetComputeRootShaderResourceView(1, src.GetGPUVirtualAddress());
            list.SetComputeRootShaderResourceView(2, runs.GetGPUVirtualAddress());
            list.SetComputeRootUnorderedAccessView(3, dst.GetGPUVirtualAddress());
            list.Dispatch(count.div_ceil(64), 1, 1);
            let barrier = D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
                ..Default::default()
            };
            list.ResourceBarrier(&[barrier]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_ranges_are_cut_into_chunks() {
        let t = table(&[(10, 10 + 600), (5000, 5003)]).unwrap();
        assert_eq!(t, [10, 256, 266, 256, 522, 88, 5000, 3]);
        assert!(table(&[(0, 1 << 33)]).is_none());
    }

    /// On the host's card: a card side holding "shader" bytes everywhere,
    /// a CPU side holding "title" bytes; after the pass exactly the runs'
    /// bytes are the title's, whatever their alignment, and every other
    /// byte (also in the dwords the runs share) is still the shader's.
    #[test]
    fn only_the_runs_bytes_change() {
        let Some(host) = crate::host::host() else {
            return; // no host GPU here
        };
        let Some(pass) = get() else {
            panic!("the scatter pass builds on this host");
        };
        const SIZE: usize = 4096;
        let ranges = [(0usize, 4usize), (5, 6), (7, 13), (17, 18), (19, 20), (21, 300), (1001, 1003), (4092, 4096)];
        let (src, src_data) = host.shared_buffer(SIZE as u64).expect("cpu side");
        let (dst_up, dst_data) = host.shared_buffer(SIZE as u64).expect("card side source");
        let (back, back_data) = host.shared_buffer(SIZE as u64).expect("read-back");
        let dst = host.uav_buffer(SIZE as u64).expect("card side");
        let words = table(&ranges).unwrap();
        let (runs, runs_data) = host.shared_buffer((words.len() * 4) as u64).expect("runs");
        // SAFETY: mapped buffers of these sizes.
        unsafe {
            for i in 0..SIZE {
                *src_data.add(i) = 0xA0 ^ (i as u8);
                *dst_data.add(i) = 0x05 ^ (i as u8).rotate_left(3);
            }
            std::ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>(), runs_data, words.len() * 4);
        }
        // SAFETY: a list of our own on the host's device and queue, waited
        // for before anything it uses is dropped.
        unsafe {
            let device = &host.device;
            let allocator: ID3D12CommandAllocator = device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
            let list: ID3D12GraphicsCommandList =
                device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None).unwrap();
            let transition = |r: &ID3D12Resource, from, to| D3D12_RESOURCE_BARRIER {
                Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                Anonymous: D3D12_RESOURCE_BARRIER_0 {
                    Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                        pResource: std::mem::transmute_copy(r),
                        Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                        StateBefore: from,
                        StateAfter: to,
                    }),
                },
                ..Default::default()
            };
            list.CopyBufferRegion(&dst, 0, &dst_up, 0, SIZE as u64);
            list.ResourceBarrier(&[transition(&dst, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_UNORDERED_ACCESS)]);
            pass.record(&list, &src, &dst, &runs, (words.len() / 2) as u32);
            list.ResourceBarrier(&[transition(&dst, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
            list.CopyBufferRegion(&back, 0, &dst, 0, SIZE as u64);
            list.Close().unwrap();
            let queue: ID3D12CommandQueue = device
                .CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC { Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default() })
                .unwrap();
            queue.ExecuteCommandLists(&[Some(windows::core::Interface::cast(&list).unwrap())]);
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
            queue.Signal(&fence, 1).unwrap();
            while fence.GetCompletedValue() < 1 {
                std::thread::yield_now();
            }
            assert!(device.GetDeviceRemovedReason().is_ok(), "device removed");
            let got = std::slice::from_raw_parts(back_data, SIZE);
            for (i, &b) in got.iter().enumerate() {
                let in_run = ranges.iter().any(|&(a, e)| (a..e).contains(&i));
                let want = if in_run { 0xA0 ^ (i as u8) } else { 0x05 ^ (i as u8).rotate_left(3) };
                assert_eq!(b, want, "byte {i} ({})", if in_run { "in a run" } else { "outside the runs" });
            }
        }
    }
}
