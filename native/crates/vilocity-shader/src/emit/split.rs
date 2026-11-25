//! Split wave64 on 32-lane host GPUs: a console wave as two host waves of
//! 32 that exchange through group-shared memory; a child module of `lower`
//! so it shares the emitter's state.

use super::*;

impl<'a> Lower<'a> {
    /// Mesh: the thread's index in the group, which is its vertex and
    /// primitive index.
    pub(super) fn mesh_thread(&mut self) -> V {
        if let Some(f) = self.flat {
            return f;
        }
        let e = self.s.entry;
        let f = self.s.op(
            e,
            "flattenedThreadIdInGroup.i32",
            Attr::ReadNone,
            self.s.t.i32,
            96,
            &[],
        );
        self.flat = Some(f);
        f
    }

    /// The console wave size.
    pub(super) fn wave(&self) -> u32 {
        if self.x.native.wave64 { 64 } else { 32 }
    }

    /// Threads in the compute group.
    pub(super) fn group_threads(&self) -> u32 {
        self.s.threads.map_or(0, |t| t.iter().product())
    }

    /// Split: the console wave has an upper half (more than 32 threads).
    pub(super) fn paired(&self) -> bool {
        self.group_threads() > 32
    }

    /// Split: (half, console lane) of this thread. A group is one console
    /// wave: two host waves of 32 lanes, whichever threads the host put in
    /// each, since a group's threads are interchangeable (`thread_id`
    /// follows the console lane).
    // shortcut: a partial host wave is taken to fill lanes 0..n, as GPUs
    // launch them.
    pub(super) fn halves(&mut self) -> (V, V) {
        if let Some(h) = self.split_lanes {
            return h;
        }
        if self.group_threads() > 64 {
            return self.claim_halves();
        }
        let e = self.s.entry;
        let i32t = self.s.t.i32;
        let lane = self
            .s
            .op(e, "waveGetLaneIndex", Attr::ReadOnly, i32t, 111, &[]);
        let n = self.group_threads();
        let (z, one) = (self.s.m.i32(0), self.s.m.i32(1));
        let lower_half = if n <= 32 {
            None
        } else if n < 64 {
            // The full wave is the lower half, the partial one the upper.
            let t = self.s.m.bool(true);
            let all = self.ballot_word(e, t);
            let count = self
                .s
                .op(e, "unaryBits.i32", Attr::ReadNone, i32t, 31, &[all]);
            let k32 = self.s.m.i32(32);
            Some(self.s.m.cmp(e, pred::EQ, count, k32))
        } else {
            // Two full waves: the one holding thread 0 is the lower half.
            let flat = self.s.op(
                e,
                "flattenedThreadIdInGroup.i32",
                Attr::ReadNone,
                i32t,
                96,
                &[],
            );
            let is0 = self.s.m.cmp(e, pred::EQ, flat, z);
            let b = self.ballot_word(e, is0);
            Some(self.s.m.cmp(e, pred::NE, b, z))
        };
        let half = match lower_half {
            Some(low) => self.s.m.select(e, low, z, one),
            None => z,
        };
        let k5 = self.s.m.i32(5);
        let base = self.s.m.bin(e, Bin::Shl, half, k5);
        let console = self.s.m.bin(e, Bin::Add, base, lane);
        self.split_lanes = Some((half, console));
        self.console_thread = Some(console);
        self.console_wave = Some(z);
        (half, console)
    }

    /// Split, several console waves per group: [`Self::halves`] by claim.
    /// Each host wave takes the next 32 console threads from a group-shared
    /// counter as it arrives (a partial host wave the last ones), so the
    /// console's waves are whole whichever threads the host put together.
    /// The group's exchange counters start at 0: every thread clears one
    /// before a group barrier, at the start, where every thread is.
    pub(super) fn claim_halves(&mut self) -> (V, V) {
        let e = self.s.entry;
        let i32t = self.s.t.i32;
        let n = self.group_threads();
        let sync = self.sync_array();
        let flat = self.s.op(
            e,
            "flattenedThreadIdInGroup.i32",
            Attr::ReadNone,
            i32t,
            96,
            &[],
        );
        // Entries: the claim counter, then two arrival counters per console
        // wave, then one every other thread clears.
        let last = self.s.m.i32(1 + 2 * n.div_ceil(64));
        let small = self.s.m.cmp(e, pred::IULT, flat, last);
        let at = self.s.m.select(e, small, flat, last);
        let p = self.s.m.element(e, sync, at);
        let z = self.s.m.i32(0);
        self.s.m.store(e, p, z);
        let mode = self.s.m.i32(1 | 8);
        let void = self.s.t.void;
        self.s.op(e, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
        let lane = self
            .s
            .op(e, "waveGetLaneIndex", Attr::ReadOnly, i32t, 111, &[]);
        let t = self.s.m.bool(true);
        let all = self.ballot_word(e, t);
        let count = self
            .s
            .op(e, "unaryBits.i32", Attr::ReadNone, i32t, 31, &[all]);
        let k32 = self.s.m.i32(32);
        let full = self.s.m.cmp(e, pred::EQ, count, k32);
        let i1 = self.s.t.i1;
        let first = self
            .s
            .op(e, "waveIsFirstLane", Attr::NoUnwind, i1, 110, &[]);
        let takes = self.s.m.bin(e, Bin::And, first, full);
        let add = self.s.m.select(e, takes, k32, z);
        let p0 = self.s.m.element(e, sync, z);
        let old = self.s.m.atomic(e, Rmw::Add, p0, add);
        let base = self
            .s
            .op(e, "waveReadLaneFirst.i32", Attr::NoUnwind, i32t, 118, &[old]);
        let tail = self.s.m.i32(n & !31);
        let base = self.s.m.select(e, full, base, tail);
        let console = self.s.m.bin(e, Bin::Add, base, lane);
        let (k5, k6, one, k63) = (
            self.s.m.i32(5),
            self.s.m.i32(6),
            self.s.m.i32(1),
            self.s.m.i32(63),
        );
        let wave = self.s.m.bin(e, Bin::LShr, base, k6);
        let half = self.s.m.bin(e, Bin::LShr, base, k5);
        let half = self.s.m.bin(e, Bin::And, half, one);
        let lane64 = self.s.m.bin(e, Bin::And, console, k63);
        self.s.flags |= flags::WAVE_OPS;
        self.split_lanes = Some((half, lane64));
        self.console_thread = Some(console);
        self.console_wave = Some(wave);
        (half, lane64)
    }

    /// Split, several console waves per group: `[claim, arrivals of each
    /// console wave's halves, spare]` in group-shared memory.
    pub(super) fn sync_array(&mut self) -> V {
        if let Some(g) = self.xch {
            return g;
        }
        let waves = self.group_threads().div_ceil(64);
        // The arrival counters, then the slots: two per half and wave.
        let arr = self.s.m.array(2 + 2 * waves + 4 * waves, self.s.t.i32);
        let g = self.s.m.global("vilocity_sync", arr, 3);
        self.xch = Some(g);
        g
    }

    /// Split, several console waves per group: [`Self::exchange`] between
    /// the two halves of this console wave only (other waves may be
    /// elsewhere). Each half has an arrival counter it alone raises: k
    /// exchanges done. A half writes its word to slot k & 1 of its pair,
    /// fences, raises its counter to k + 1 and waits until the other's is
    /// there too, then reads the other's word. The other half can be one
    /// exchange ahead at most, so it never rewrites a slot still being
    /// read. The wait is bounded ([`LOOP_LIMIT`] polls): a pair that lost
    /// step reads stale words rather than hang the GPU.
    pub(super) fn exchange_far(&mut self, word: V, missing: u32) -> (V, V) {
        let (half, _) = self.halves();
        let wave = self.console_wave.expect("claimed");
        let sync = self.sync_array();
        let waves = self.group_threads().div_ceil(64);
        let n = self.group_threads();
        let c = self.cur;
        let m = |l: &mut Self, k: u32| l.s.m.i32(k);
        let (z, one, two, k32, k64) = (m(self, 0), m(self, 1), m(self, 2), m(self, 32), m(self, 64));
        // Counters at 1 + 2w + h; slots after them at 2 + 2W + (2w + p) * 2 + h.
        let w2 = self.s.m.bin(c, Bin::Mul, wave, two);
        let mine = self.s.m.bin(c, Bin::Add, w2, half);
        let mine = self.s.m.bin(c, Bin::Add, mine, one);
        let other_half = self.s.m.bin(c, Bin::Xor, half, one);
        let theirs = self.s.m.bin(c, Bin::Add, w2, other_half);
        let theirs = self.s.m.bin(c, Bin::Add, theirs, one);
        let my_count = self.s.m.element(c, sync, mine);
        let k = self.s.m.atomic(c, Rmw::Add, my_count, z);
        let parity = self.s.m.bin(c, Bin::And, k, one);
        let pair = self.s.m.bin(c, Bin::Add, w2, parity);
        let pair = self.s.m.bin(c, Bin::Mul, pair, two);
        let slots = self.s.m.i32(2 + 2 * waves);
        let pair = self.s.m.bin(c, Bin::Add, pair, slots);
        let my_slot = self.s.m.bin(c, Bin::Add, pair, half);
        let their_slot = self.s.m.bin(c, Bin::Add, pair, other_half);
        #[cfg(test)]
        let c = if DELAY_UPPER.load(std::sync::atomic::Ordering::Relaxed) {
            self.dawdle(half, sync)
        } else {
            c
        };
        let p = self.s.m.element(c, sync, my_slot);
        self.s.m.atomic(c, Rmw::Xchg, p, word);
        self.fence();
        let next = self.s.m.bin(c, Bin::Add, k, one);
        self.s.m.atomic(c, Rmw::UMax, my_count, next);
        // The upper half of the last console wave may not exist.
        let start = self.s.m.bin(c, Bin::Mul, wave, k64);
        let upper = self.s.m.bin(c, Bin::Add, start, k32);
        let nk = self.s.m.i32(n);
        let paired = self.s.m.cmp(c, pred::IULT, upper, nk);
        // Wait for the other half's counter.
        let (head, done) = (self.s.m.new_block(), self.s.m.new_block());
        self.s.m.terminate(c, Term::Br(head));
        let polls = self.s.m.phi(head, self.s.t.i32);
        let their_count = self.s.m.element(head, sync, theirs);
        let seen = self.s.m.atomic(head, Rmw::Add, their_count, z);
        let arrived = self.s.m.cmp(head, pred::IUGE, seen, next);
        let no = self.s.m.bool(false);
        let alone = self.s.m.cmp(head, pred::EQ, paired, no);
        let go = self.s.m.bin(head, Bin::Or, arrived, alone);
        let polled = self.s.m.bin(head, Bin::Add, polls, one);
        let limit = self.s.m.i32(LOOP_LIMIT);
        let over = self.s.m.cmp(head, pred::IUGE, polled, limit);
        let leave = self.s.m.bin(head, Bin::Or, go, over);
        self.s.m.set_phi(polls, vec![(z, c), (polled, head)]);
        self.s.m.terminate(head, Term::CondBr(leave, done, head));
        self.cur = done;
        self.fence();
        let q = self.s.m.element(done, sync, their_slot);
        let other = self.s.m.atomic(done, Rmw::Add, q, z);
        let missing = self.s.m.i32(missing);
        let other = self.s.m.select(done, paired, other, missing);
        let low = self.s.m.cmp(done, pred::EQ, half, z);
        let lo = self.s.m.select(done, low, word, other);
        let hi = self.s.m.select(done, low, other, word);
        (lo, hi)
    }

    /// Tests ([`DELAY_UPPER`]): the upper half spins a while on the spare
    /// sync entry. Returns the block after it.
    #[cfg(test)]
    pub(super) fn dawdle(&mut self, half: V, sync: V) -> usize {
        let c = self.cur;
        let (head, done) = (self.s.m.new_block(), self.s.m.new_block());
        self.s.m.terminate(c, Term::Br(head));
        let i = self.s.m.phi(head, self.s.t.i32);
        let spare = self.s.m.i32(1 + 2 * self.group_threads().div_ceil(64));
        let p = self.s.m.element(head, sync, spare);
        let one = self.s.m.i32(1);
        self.s.m.atomic(head, Rmw::Add, p, one);
        let next = self.s.m.bin(head, Bin::Add, i, one);
        let z = self.s.m.i32(0);
        let upper = self.s.m.cmp(head, pred::NE, half, z);
        let k = self.s.m.i32(1 << 14);
        let more = self.s.m.cmp(head, pred::IULT, next, k);
        let more = self.s.m.bin(head, Bin::And, upper, more);
        self.s.m.set_phi(i, vec![(z, c), (next, head)]);
        self.s.m.terminate(head, Term::CondBr(more, head, done));
        self.cur = done;
        done
    }

    /// A group-shared memory fence (no sync: legal where waves diverge).
    pub(super) fn fence(&mut self) {
        let mode = self.s.m.i32(8);
        let void = self.s.t.void;
        self.s
            .op(self.cur, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
    }

    /// Split, several console waves per group: the ballots instruction `i`
    /// of block `b` reads as numbers, taken before it (see [`Prefetch`]).
    pub(super) fn take_prefetched(&mut self, b: usize, i: usize) {
        let Some(masks) = self.prefetch.get(&(b, i)) else {
            return;
        };
        for &lo in masks {
            let key = (lo, b, i);
            if self.prefetched.contains_key(&key) {
                continue;
            }
            let ballot = match self.x.st.masks.get(&lo).copied() {
                None => Err(LANE_MASK),
                Some(m) => self.boolean(m, self.cur).map(|bit| {
                    let w = self.ballot_word(self.cur, bit);
                    self.exchange(w, 0)
                }),
            };
            self.prefetched.insert(key, ballot);
        }
    }

    /// The host wave's ballot of `bit` (32 lanes: the first dword).
    pub(super) fn ballot_word(&mut self, at: usize, bit: V) -> V {
        let i32t = self.s.t.i32;
        let four = self.s.m.named_struct("dx.types.fouri32", vec![i32t; 4]);
        let r = self
            .s
            .op(at, "waveActiveBallot", Attr::NoUnwind, four, 116, &[bit]);
        self.s.flags |= flags::WAVE_OPS;
        self.s.m.extract(at, r, 0)
    }

    /// A group barrier with a group-shared fence.
    pub(super) fn group_barrier(&mut self) {
        let mode = self.s.m.i32(1 | 8);
        let void = self.s.t.void;
        self.s
            .op(self.cur, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
    }

    /// Split: each half gives `word` (the same in all its lanes); returns
    /// (the lower half's, the upper half's), `missing` for an upper half
    /// that does not exist. Both halves run the same blocks (every console
    /// branch is the whole wave's), so both reach every exchange and the
    /// barriers. One console wave per group: stays in the current block;
    /// several: ends in a new one ([`Self::exchange_far`]).
    pub(super) fn exchange(&mut self, word: V, missing: u32) -> (V, V) {
        if self.group_threads() > 64 {
            return self.exchange_far(word, missing);
        }
        if !self.paired() {
            return (word, self.s.m.i32(missing));
        }
        let (half, _) = self.halves();
        let g = match self.xch {
            Some(g) => g,
            None => {
                let arr = self.s.m.array(2, self.s.t.i32);
                let g = self.s.m.global("vilocity_xch", arr, 3);
                self.xch = Some(g);
                g
            }
        };
        // Every lane of the half writes the same word: an atomic swap keeps
        // that free of races without a branch.
        let p = self.s.m.element(self.cur, g, half);
        self.s.m.atomic(self.cur, Rmw::Xchg, p, word);
        self.group_barrier();
        let mut words = [word; 2];
        for (k, w) in words.iter_mut().enumerate() {
            let k = self.s.m.i32(k as u32);
            let p = self.s.m.element(self.cur, g, k);
            *w = self.s.m.load(self.cur, p);
        }
        // The slots are written again only after both halves read them.
        self.group_barrier();
        (words[0], words[1])
    }

    /// Split: `x` of console lane `lane` (the same in all lanes).
    pub(super) fn read_console_lane(&mut self, x: V, lane: V) -> V {
        let mine = self.read_lane(x, lane);
        let (lo, hi) = self.exchange(mine, 0);
        let k32 = self.s.m.i32(32);
        let z = self.s.m.i32(0);
        let upper = self.s.m.bin(self.cur, Bin::And, lane, k32);
        let upper = self.s.m.cmp(self.cur, pred::NE, upper, z);
        self.s.m.select(self.cur, upper, hi, lo)
    }
}

/// A buffer or image store whose value a later load of the console wave may read.
fn buffer_store(name: &str) -> bool {
    ["buffer_store", "tbuffer_store", "global_store", "flat_store", "image_store"]
        .iter()
        .any(|p| name.starts_with(p))
}

/// A buffer or image load that may read what the other half of the console wave stored.
fn buffer_load(name: &str) -> bool {
    ["buffer_load", "tbuffer_load", "global_load", "flat_load", "image_load"]
        .iter()
        .any(|p| name.starts_with(p))
}

/// Since the halves last met: a store (1), a load (2).
const STORED: u8 = 1;
const LOADED: u8 = 2;

/// Whether the halves meet before `name`, with `since` what came since they last met; then
/// what has come since.
fn buffer_step(name: &str, runs: bool, since: u8) -> (bool, u8) {
    let (load, store) = (buffer_load(name), buffer_store(name));
    // a load after a store reads it; a store after a load must not overwrite it first
    let meet = runs && (load && since & STORED != 0 || store && since & LOADED != 0);
    let since = if meet { 0 } else { since };
    (meet, since | u8::from(store) * STORED | u8::from(load) * LOADED)
}

/// Buffer memory a console wave64 compute program writes and reads back within its wave: the
/// console's 64 lanes run in step, so the title needs no barrier between a store and a load of
/// the same memory (either order); two host waves of 32 race there (shadPS4 #5310: flickering
/// effects on NVIDIA). The halves meet (a group barrier with a UAV fence) before a load a store
/// may precede, and before a store a load may precede, since they last met, in blocks every
/// lane runs. Per IR block: whether every lane runs it, and what may come before its start
/// ([`STORED`], [`LOADED`]). `None` where nothing needs it. `VILOCITY_BUFFER_MEET=0` leaves it
/// out.
/// shortcut: groups of one console wave only (33 to 64 threads), where a barrier in a block
/// every lane runs is reached by the whole group; stores and loads in lane branches are not
/// ordered.
pub(super) fn buffer_meetings(p: &Program, st: &Structured, wave64: bool, split: bool, threads: Option<[u32; 3]>) -> Option<(Vec<bool>, Vec<u8>)> {
    let group: u32 = threads?.iter().product();
    let off = std::env::var_os("VILOCITY_BUFFER_MEET").is_some_and(|v| v == "0");
    if off || !wave64 || !(split || host_max_wave() == 32) || !(33..=64).contains(&group) || super::super::group_sync() == 0 {
        return None;
    }
    let names = || p.blocks.iter().flat_map(|b| b.insts.iter()).map(|i| i.name());
    if !names().any(buffer_store) || !names().any(buffer_load) {
        return None;
    }
    let all = all_lane_blocks(st)?;
    let runs = |b: usize| all.get(b).copied().unwrap_or(false);
    let out = |b: usize, since: u8| {
        p.blocks[b].insts.iter().fold(since, |since, i| buffer_step(i.name(), runs(b), since).1)
    };
    let mut since_in = vec![0u8; p.blocks.len()];
    let mut changed = true;
    while changed {
        changed = false;
        for b in 0..p.blocks.len() {
            let came = p.blocks[b].preds.iter().fold(since_in[b], |a, &q| a | out(q, since_in[q]));
            if came != since_in[b] {
                since_in[b] = came;
                changed = true;
            }
        }
    }
    Some((all, since_in))
}

impl<'a> Lower<'a> {
    /// Before instruction `name` of IR block `b` ([`buffer_meetings`]): the host waves meet
    /// where the other half's store or load must be done first.
    pub(super) fn buffer_meeting(&mut self, b: usize, name: &str) {
        let Some((all, since_in)) = &self.buffer_meet else {
            return;
        };
        let (runs, entered) = (all.get(b).copied().unwrap_or(false), since_in[b]);
        if self.buffer_at.0 != b {
            self.buffer_at = (b, entered);
        }
        let (meet, since) = buffer_step(name, runs, self.buffer_at.1);
        self.buffer_at.1 = since;
        if meet {
            #[cfg(test)]
            buffer_tests::MET.with(|m| m.set(m.get() + 1));
            // a wait for the group, with UAV and shared-memory fences
            let mode = self.s.m.i32(1 | 2 | 8);
            let void = self.s.t.void;
            self.s.op(self.cur, "barrier", Attr::NoDuplicate, void, 80, &[mode]);
        }
    }
}

#[cfg(test)]
mod buffer_tests {
    use super::{LOADED, STORED, buffer_step};
    use crate::emit::split_tests::{V_ADD_NC_U32, V_LSHLREV_B32, check_in, k, program_in, try_lower, vop2};

    thread_local! {
        /// Meetings the lowering on this thread added.
        pub(super) static MET: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    #[test]
    fn a_half_reads_what_the_other_half_stored() {
        // Each thread stores tid + 64 at dword tid, then loads dword tid ^ 32: the other
        // half's. The console's lanes run in step; the split form's halves meet between the
        // store and the load, and between the load and the final store.
        const BUFFER_STORE_DWORD: u8 = 0x1C;
        const BUFFER_LOAD_DWORD: u8 = 0x0C;
        const V_XOR_B32: u8 = 0x1D;
        let mubuf = |op: u8, vdata: u8, vaddr: u8| {
            [0x38 << 26 | (op as u32) << 18 | 1 << 12, 0x80 << 24 | 2 << 16 | (vdata as u32) << 8 | vaddr as u32]
        };
        let mut body = vec![vop2(V_LSHLREV_B32, 2, k(2), 0), vop2(V_ADD_NC_U32, 3, k(64), 0)];
        body.extend_from_slice(&mubuf(BUFFER_STORE_DWORD, 3, 2));
        body.extend_from_slice(&[vop2(V_XOR_B32, 4, k(32), 0), vop2(V_LSHLREV_B32, 4, k(2), 4)]);
        body.extend_from_slice(&mubuf(BUFFER_LOAD_DWORD, 1, 4));
        let native = program_in(&body, [64, 1, 1], 0);
        let met = |split| {
            MET.with(|m| m.set(0));
            try_lower(&native, split).expect("lower");
            MET.with(|m| m.get())
        };
        assert_eq!(met(true), 2, "the split form's halves meet twice");
        assert_eq!(met(false), 0, "one 64-lane wave needs no meeting");
        check_in(&native, |t| (t ^ 32) + 64);
    }

    #[test]
    fn the_halves_meet_between_a_store_and_a_load_either_way() {
        // a load after a store meets, and then has met
        assert_eq!(buffer_step("buffer_load_dword", true, STORED), (true, LOADED));
        // a store after a load meets
        assert_eq!(buffer_step("global_store_dword", true, LOADED), (true, STORED));
        // loads after loads, stores after stores, and other instructions do not
        assert_eq!(buffer_step("image_load", true, LOADED), (false, LOADED));
        assert_eq!(buffer_step("buffer_store_dword", true, STORED), (false, STORED));
        assert_eq!(buffer_step("v_add_nc_u32", true, STORED | LOADED), (false, STORED | LOADED));
        // where not every lane runs, no barrier: what came is carried on
        assert_eq!(buffer_step("buffer_load_dword", false, STORED), (false, STORED | LOADED));
    }
}
