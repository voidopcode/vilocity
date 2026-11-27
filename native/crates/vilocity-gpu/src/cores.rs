//! Which processors our own threads run on, where a processor has two kinds
//! of cores. `cpu_sets` and `keep_on_fast_cores` are fxceless/victory-fork's
//! (`framelog.rs`, commit 0b0074e0, GPL-3.0-or-later, as this).

/// The system's processor sets of group 0: each one's id and its efficiency
/// class (higher is faster).
fn cpu_sets() -> Vec<(u32, u8)> {
    cpu_sets_placed().into_iter().map(|s| (s.0, s.1)).collect()
}

/// [`cpu_sets`], and each one's logical processor.
fn cpu_sets_placed() -> Vec<(u32, u8, u8)> {
    use windows_sys::Win32::System::SystemInformation::{GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut bytes = 0u32;
    // SAFETY: a size query, then the call into a buffer of that size.
    unsafe { GetSystemCpuSetInformation(std::ptr::null_mut(), 0, &mut bytes, GetCurrentProcess(), 0) };
    let mut buffer = vec![0u64; (bytes as usize).div_ceil(8).max(1)];
    // SAFETY: as above; the buffer holds `bytes` bytes, aligned for the records.
    let ok = unsafe { GetSystemCpuSetInformation(buffer.as_mut_ptr().cast(), bytes, &mut bytes, GetCurrentProcess(), 0) };
    let mut sets = Vec::new();
    let mut at = 0usize;
    while ok != 0 && at + size_of::<SYSTEM_CPU_SET_INFORMATION>() <= bytes as usize {
        // SAFETY: a record inside the buffer; each says its own size.
        let info = unsafe { &*(buffer.as_ptr().cast::<u8>().add(at).cast::<SYSTEM_CPU_SET_INFORMATION>()) };
        if info.Size == 0 {
            break;
        }
        // SAFETY: the record's type is CpuSet (the only one there is).
        let set = unsafe { &info.Anonymous.CpuSet };
        if set.Group == 0 {
            sets.push((set.Id, set.EfficiencyClass, set.LogicalProcessorIndex));
        }
        at += info.Size as usize;
    }
    sets
}

/// The sets of the fastest class, if there are slower ones too.
fn fast_of(sets: &[(u32, u8)]) -> Vec<u32> {
    let best = sets.iter().map(|c| c.1).max().unwrap_or(0);
    let fast: Vec<u32> = sets.iter().filter(|c| c.1 == best).map(|c| c.0).collect();
    if fast.len() == sets.len() { Vec::new() } else { fast }
}

/// `VILOCITY_FAST_CORES`: 0: no thread asks; 1: the threads a frame waits for
/// one by one; 2 (the default): the threads they share their work with too;
/// 3: those that work beside the rest as well (lists translated, descriptors
/// made): more threads by then than the fast cores run at a time, while the
/// slow ones stood idle.
fn stage() -> u32 {
    static STAGE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *STAGE.get_or_init(|| std::env::var("VILOCITY_FAST_CORES").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(2))
}

/// Asks the system to run the calling thread, which a frame waits for, on
/// the fast cores only, where the processor has two kinds. That fork's submit
/// thread was found on a slow one in 1,529 looks of 24,021. (A selection of
/// processors is a preference the scheduler keeps unless nothing of it is
/// free.) The title's threads are not asked for.
pub(crate) fn keep_on_fast_cores(what: &str) {
    ask(what, 1);
}

/// [`keep_on_fast_cores`] for one of the threads a submission's preparing or
/// a landing shares its work with.
pub(crate) fn keep_helper_on_fast_cores(what: &str) {
    ask(what, 2);
}

/// [`keep_on_fast_cores`] for one of many threads whose work is ahead of what
/// a frame waits for.
pub(crate) fn keep_many_on_fast_cores(what: &str) {
    ask(what, 3);
}

fn ask(what: &str, from: u32) {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadSelectedCpuSets};
    if stage() < from {
        return;
    }
    static FAST: std::sync::OnceLock<(Vec<u32>, usize)> = std::sync::OnceLock::new();
    let (fast, of) = FAST.get_or_init(|| {
        let sets = cpu_sets();
        (fast_of(&sets), sets.len())
    });
    // All alike (or nothing known): nothing to ask for.
    if fast.is_empty() {
        return;
    }
    // SAFETY: our own thread and a list of the system's own set ids.
    let ok = unsafe { SetThreadSelectedCpuSets(GetCurrentThread(), fast.as_ptr(), fast.len() as u32) };
    if ok == 0 {
        tracing::warn!(what, error = %std::io::Error::last_os_error(), "the thread could not be kept on the fast cores");
    } else {
        tracing::info!(what, fast = fast.len(), of, "the thread is kept on the fast cores (VILOCITY_FAST_CORES=0: not)");
    }
}

/// (Vilocity) The processor's dies with a third-level cache each (two in a
/// Ryzen 9): the processor sets of the die with the biggest cache (the 3D
/// V-Cache one; the first of equals), and those of the other dies. `None`: one
/// die, cores of two kinds, a side of under 16 threads (not measured), or
/// `VILOCITY_DIES=0`.
///
/// The title's own threads and the threads a frame waits for run on the first
/// ([`title_on_its_die`]: the process's default), our many worker threads on
/// the others ([`keep_beside`]): they no longer take each other's cores. On a
/// Ryzen 9 9950X3D with an RTX 5080, switched every 10 s from outside in one run
/// of a standing scene at 4,300 draws: a median of 34.8 ms a frame against
/// 37.1. (The process's priority class, the same way: 36.2 against 36.1.)
fn dies() -> Option<&'static (Vec<u32>, Vec<u32>)> {
    static DIES: std::sync::OnceLock<Option<(Vec<u32>, Vec<u32>)>> = std::sync::OnceLock::new();
    DIES.get_or_init(|| {
        if std::env::var_os("VILOCITY_DIES").is_some_and(|v| v == "0") {
            return None;
        }
        dies_of(&third_level_caches(), &cpu_sets_placed())
    })
    .as_ref()
}

/// [`dies`] of the third-level caches (bytes, their logical processors' mask)
/// and the processor sets (id, class, logical processor).
fn dies_of(caches: &[(u32, u64)], sets: &[(u32, u8, u8)]) -> Option<(Vec<u32>, Vec<u32>)> {
    let &(_, title) = caches.iter().max_by_key(|c| (c.0, std::cmp::Reverse(c.1)))?;
    let of = |on_title: bool| -> Vec<u32> { sets.iter().filter(|s| (title >> s.2 & 1 != 0) == on_title).map(|s| s.0).collect() };
    let (first, rest) = (of(true), of(false));
    let alike = sets.iter().all(|s| s.1 == sets[0].1);
    (caches.len() >= 2 && alike && first.len() >= 16 && rest.len() >= 16).then_some((first, rest))
}

/// The third-level caches of processor group 0: bytes, and the mask of the
/// logical processors that share each.
fn third_level_caches() -> Vec<(u32, u64)> {
    use windows_sys::Win32::System::SystemInformation::{GetLogicalProcessorInformationEx, RelationCache};
    let mut bytes = 0u32;
    // SAFETY: a size query, then the call into a buffer of that size.
    unsafe { GetLogicalProcessorInformationEx(RelationCache, std::ptr::null_mut(), &mut bytes) };
    let mut buffer = vec![0u64; (bytes as usize).div_ceil(8).max(1)];
    // SAFETY: as above; the buffer holds `bytes` bytes, aligned for the records.
    let ok = unsafe { GetLogicalProcessorInformationEx(RelationCache, buffer.as_mut_ptr().cast(), &mut bytes) };
    // SAFETY: the buffer's own bytes.
    let data: &[u8] = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast(), (bytes as usize).min(buffer.len() * 8)) };
    // By their bytes (the record's layout is the same in every SDK): a record's
    // size at 4; the cache's level at 8 and its bytes at 12; its processors'
    // mask at 40 and their group at 48.
    let mut caches = Vec::new();
    let mut at = 0;
    while ok != 0 && at + 50 <= data.len() {
        let word = |o: usize| u32::from_le_bytes([data[at + o], data[at + o + 1], data[at + o + 2], data[at + o + 3]]);
        let size = word(4) as usize;
        if size == 0 {
            break;
        }
        let mask = u64::from(word(40)) | u64::from(word(44)) << 32;
        if data[at + 8] == 3 && data[at + 48] == 0 && data[at + 49] == 0 {
            caches.push((word(12), mask));
        }
        at += size;
    }
    caches
}

/// (Vilocity) The title's threads, and every thread of ours that does not ask
/// for other processors, run on the die with the biggest cache from now on, if
/// the processor has two ([`dies`]). For the process that runs the title.
pub fn title_on_its_die() {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessDefaultCpuSets};
    let Some((title, rest)) = dies() else { return };
    // SAFETY: our own process and a list of the system's own set ids.
    let ok = unsafe { SetProcessDefaultCpuSets(GetCurrentProcess(), title.as_ptr(), title.len() as u32) };
    if ok == 0 {
        tracing::warn!(error = %std::io::Error::last_os_error(), "the title's threads could not be kept on one die");
    } else {
        tracing::info!(
            title = title.len(),
            workers = rest.len(),
            "two dies: the title's threads and the frame's on the one with the biggest cache, our workers on the other (VILOCITY_DIES=0: not)"
        );
    }
}

/// (Vilocity) Asks the system to run the calling thread, one of our many
/// workers, on the other die than the title's ([`dies`]).
pub fn keep_beside(what: &str) {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadSelectedCpuSets};
    let Some((_, rest)) = dies() else { return };
    // SAFETY: our own thread and a list of the system's own set ids.
    if unsafe { SetThreadSelectedCpuSets(GetCurrentThread(), rest.as_ptr(), rest.len() as u32) } == 0 {
        tracing::warn!(what, error = %std::io::Error::last_os_error(), "the thread could not be kept beside the title's die");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_dies_are_split_at_the_biggest_cache() {
        let sets: Vec<(u32, u8, u8)> = (0..32).map(|i| (256 + i as u32, 0, i)).collect();
        let (low, high) = (0xFFFFu64, 0xFFFF_0000u64);
        let ids = |r: std::ops::Range<u32>| r.map(|i| 256 + i).collect::<Vec<u32>>();
        // The die with the bigger cache is the title's, wherever it is.
        assert_eq!(dies_of(&[(96 << 20, low), (32 << 20, high)], &sets), Some((ids(0..16), ids(16..32))));
        assert_eq!(dies_of(&[(32 << 20, low), (96 << 20, high)], &sets), Some((ids(16..32), ids(0..16))));
        // Alike: the first.
        assert_eq!(dies_of(&[(32 << 20, high), (32 << 20, low)], &sets), Some((ids(0..16), ids(16..32))));
        // One die, cores of two kinds, or small dies: no split.
        assert_eq!(dies_of(&[(96 << 20, u32::MAX as u64)], &sets), None);
        assert_eq!(dies_of(&[], &sets), None);
        let mut two_kinds = sets.clone();
        two_kinds[20].1 = 1;
        assert_eq!(dies_of(&[(96 << 20, low), (32 << 20, high)], &two_kinds), None);
        assert_eq!(dies_of(&[(32 << 20, 0xFFF), (32 << 20, 0xFFF000)], &sets[..24]), None);
        // This machine: whatever it has, the calls go through.
        assert!(!cpu_sets_placed().is_empty());
        let _ = third_level_caches();
        std::thread::spawn(|| keep_beside("test")).join().unwrap();
    }

    #[test]
    fn the_fast_cores_are_those_of_the_best_class_if_there_is_a_worse_one() {
        assert_eq!(fast_of(&[(256, 1), (257, 1), (258, 0)]), [256, 257]);
        assert!(fast_of(&[(256, 0), (257, 0)]).is_empty());
        assert!(fast_of(&[]).is_empty());
        assert!(!cpu_sets().is_empty());
        std::thread::spawn(|| keep_on_fast_cores("test")).join().unwrap();
    }
}
