//! First-lane reads (`v_readfirstlane_b32`), a child module of `lower` so
//! they share the emitter's state.

use super::*;

impl<'a> Lower<'a> {
    /// `x` in the first lane of this host wave where `guard` holds, and whether it has such a
    /// lane: the console reads the first lane of EXEC, and a lane that got here with EXEC clear
    /// (a guard, not a branch) is not one. With none, `x` of the wave's first lane here.
    pub(super) fn first_lane(&mut self, x: V, guard: Option<V>) -> (V, V) {
        let c = self.cur;
        let i32t = self.s.t.i32;
        self.s.flags |= flags::WAVE_OPS;
        let first = self.s.op(c, "waveReadLaneFirst.i32", Attr::NoUnwind, i32t, 118, &[x]);
        let Some(g) = guard else {
            return (first, self.s.m.bool(true));
        };
        let four = self.s.m.named_struct("dx.types.fouri32", vec![i32t; 4]);
        let r = self.s.op(c, "waveActiveBallot", Attr::NoUnwind, four, 116, &[g]);
        let (lo, hi) = (self.s.m.extract(c, r, 0), self.s.m.extract(c, r, 1));
        let (z, k32) = (self.s.m.i32(0), self.s.m.i32(32));
        // FirstbitLo (dx.op 32): the lowest set bit
        let flo = self.s.op(c, "unaryBits.i32", Attr::ReadNone, i32t, 32, &[lo]);
        let fhi = self.s.op(c, "unaryBits.i32", Attr::ReadNone, i32t, 32, &[hi]);
        let fhi = self.s.m.bin(c, Bin::Add, fhi, k32);
        let in_lo = self.s.m.cmp(c, pred::NE, lo, z);
        let in_hi = self.s.m.cmp(c, pred::NE, hi, z);
        let any = self.s.m.bin(c, Bin::Or, in_lo, in_hi);
        let lane = self.s.m.select(c, in_lo, flo, fhi);
        // no lane in EXEC: read this lane (one that is here); the read is not used
        let me = self.s.op(c, "waveGetLaneIndex", Attr::ReadOnly, i32t, 111, &[]);
        let lane = self.s.m.select(c, any, lane, me);
        let at = self.read_lane(x, lane);
        (self.s.m.select(c, any, at, first), any)
    }

    /// `v_readfirstlane_b32`'s result `first` of `own` in a pixel shader:
    /// a helper lane takes its own value instead.
    ///
    /// On the console, whole-quad mode puts helper lanes in EXEC, so a
    /// waterfall loop (read the first lane's value, run the lanes with that
    /// value, take them out of EXEC) serves helper lanes too. On the host,
    /// wave operations leave helper lanes out (DXIL before
    /// `WaveOpsIncludeHelperLanes`): once only helper lanes are left in such
    /// a loop, `WaveReadLaneFirst` reads no lane, no lane matches, and the
    /// loop runs to [`LOOP_LIMIT`] in every such lane: draws of 0.3 to 1.3 s
    /// each, then DEVICE_HUNG. With its own value a helper lane matches in
    /// the first pass and leaves; what a helper lane computes only feeds
    /// derivatives, and its stores are dropped. `VILOCITY_HELPER_OWN=0`: the
    /// first lane's (codec-d/cool_fox 30e54b4's switch).
    pub(super) fn helper_reads_own(&mut self, own: V, first: V) -> V {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !self.s.pixel || !*ON.get_or_init(|| std::env::var_os("VILOCITY_HELPER_OWN").is_none_or(|v| v != "0")) {
            return first;
        }
        let c = self.cur;
        let helper = self
            .s
            .op(c, "isHelperLane.i1", Attr::ReadOnly, self.s.t.i1, 221, &[]);
        self.s.m.select(c, helper, own, first)
    }
}

/// Where a program's constant table starts in its code: where its
/// instructions end, unless a load at a fixed offset reads below that (then
/// at `lowest`, the lowest offset any load or descriptor names). The
/// program's constants follow its instructions, and a load at a computed
/// offset reads them, not instruction bytes. The table from offset 0 was
/// the whole program: 16-18 KiB in 12 lighting programs, which NVIDIA's
/// compiler puts in each thread's local memory (GBs of the card's memory;
/// Xid 13 faults on an RTX 5090). Marvisak/VIctory#2143 by desplmfao, via
/// fxceless/victory-fork#1006.
pub(super) fn code_table_start(p: &crate::ir::Program, addresses: &[&crate::resource::Address], lowest: u32) -> u32 {
    let code_end = p
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .map(|i| i.machine.offset + 4 * i.machine.words.len())
        .max()
        .unwrap_or(0) as u32;
    let fixed_below = addresses.iter().any(|a| a.dynamic.is_none() && a.offset < code_end);
    if fixed_below { lowest } else { lowest.max(code_end & !3) }
}

#[cfg(test)]
mod code_table_tests {
    use super::code_table_start;
    use crate::{ir, resource};

    const S_ENDPGM: u32 = 0xBF81_0000;

    /// A program's constant table starts after its instructions, unless a
    /// load at a fixed offset reads below that.
    #[test]
    fn the_code_table_starts_after_the_instructions() {
        // s_nop 0, s_endpgm, then 1,000 dwords of constants.
        let mut words = vec![0xBF80_0000, S_ENDPGM];
        words.extend(0..1000u32);
        let code: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let p = ir::build(&code, false, 256).unwrap();
        let at = |offset: u32, dynamic: bool| resource::Address {
            param: resource::CODE,
            offset,
            dynamic: dynamic.then_some(ir::Value(0)),
        };
        let (computed, fixed_after, fixed_inside) = (at(0, true), at(12, false), at(4, false));
        // A load at a computed offset from the code's start: the table is
        // the constants only, not the whole program.
        assert_eq!(code_table_start(&p, &[&computed], 0), 8);
        assert_eq!(code_table_start(&p, &[&computed, &fixed_after], 0), 8);
        // A load at a fixed offset inside the instructions keeps the start.
        assert_eq!(code_table_start(&p, &[&computed, &fixed_inside], 0), 0);
        // Loads past the instructions only: from the lowest of them.
        assert_eq!(code_table_start(&p, &[&fixed_after], 12), 12);
    }
}
