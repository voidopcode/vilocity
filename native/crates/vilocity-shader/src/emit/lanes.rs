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
    /// derivatives, and its stores are dropped.
    pub(super) fn helper_reads_own(&mut self, own: V, first: V) -> V {
        if !self.s.pixel {
            return first;
        }
        let c = self.cur;
        let helper = self
            .s
            .op(c, "isHelperLane.i1", Attr::ReadOnly, self.s.t.i1, 221, &[]);
        self.s.m.select(c, helper, own, first)
    }
}
