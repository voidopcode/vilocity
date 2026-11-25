//! Global stores and atomics at an address in a root UAV; a child module
//! of `lower` so it shares the emitter's state.

use super::*;

impl<'a> Lower<'a> {
    /// A global store or atomic (`global_store_*`, `global_atomic_*`) at an
    /// address in a root UAV: its raw view at the byte offset. Bytes and
    /// halves clear and set their bits atomically (see `ds`); 64-bit
    /// atomics (`_x2`) take their value from two dwords (compare-exchange:
    /// the new value's, then the compared one's).
    pub(super) fn global_write(&mut self, inst: &Inst, b: usize, i: usize) -> Result<(), Why> {
        let Fields::Flat { dst, data, .. } = &inst.fields else {
            unreachable!()
        };
        let name = inst.name();
        let a = *self
            .scalar
            .get(&(b, i))
            .ok_or("an untracked global address")?;
        if a.param == CODE {
            return Err("a write to the program's code");
        }
        let guard = self.guard(b, i)?;
        let c = self.cur;
        let mut off = self.s.m.i32(a.offset);
        if let Some(d) = a.dynamic {
            let d = self.i32_of(d)?;
            off = self.s.m.bin(c, Bin::Add, off, d);
        }
        let src = Source::Built {
            address: a,
            bits: 0,
            words: [0, 0],
        };
        let h = self.handle((b, i), &src, false, (11 | 0x1000, 0))?;
        let iu = self.s.m.undef(self.s.t.i32);
        let d = data.as_ref().ok_or("global data")?;
        let i32t = self.s.t.i32;
        let void = self.s.t.void;
        if let Some(rest) = name.strip_prefix("global_store_") {
            let n = match rest {
                "dword" => 1,
                "dwordx2" => 2,
                "dwordx3" => 3,
                "dwordx4" => 4,
                "byte" | "short" => {
                    let bits = if rest == "byte" { 8 } else { 16 };
                    let k = |l: &mut Self, v: u32| l.s.m.i32(v);
                    let (low, eight, not_low) = (k(self, 3), k(self, 8), k(self, !3));
                    let aligned = self.s.m.bin(c, Bin::And, off, not_low);
                    let shift = self.s.m.bin(c, Bin::And, off, low);
                    let shift = self.s.m.bin(c, Bin::Mul, shift, eight);
                    let ones = k(self, (1u32 << bits) - 1);
                    let v = self.vgprs(inst, d, 1)?[0];
                    let v = self.s.m.bin(c, Bin::And, v, ones);
                    let v = self.s.m.bin(c, Bin::Shl, v, shift);
                    let mask = self.s.m.bin(c, Bin::Shl, ones, shift);
                    let all = k(self, u32::MAX);
                    let keep = self.s.m.bin(c, Bin::Xor, mask, all);
                    let at = self.begin_guarded(guard);
                    for (op, x) in [(1, keep), (2, v)] {
                        let args = [h, self.s.m.i32(op), aligned, iu, iu, x];
                        self.s
                            .op(self.cur, "atomicBinOp.i32", Attr::NoUnwind, i32t, 78, &args);
                    }
                    self.end_guarded(at, &[]);
                    return Ok(());
                }
                _ => return Err(name),
            };
            let vals = self.vgprs(inst, d, n)?;
            let mut args = vec![h, off, iu];
            args.extend(&vals);
            args.resize(3 + 4, iu);
            args.push(self.s.m.i8((1u8 << n) - 1));
            args.push(self.s.m.i32(4));
            let at = self.begin_guarded(guard);
            self.s
                .op(self.cur, "rawBufferStore.i32", Attr::NoUnwind, void, 140, &args);
            self.end_guarded(at, &[]);
            return Ok(());
        }
        let op = name.strip_prefix("global_atomic_").ok_or(name)?;
        let (op, wide) = match op.strip_suffix("_x2") {
            Some(o) => (o, true),
            None => (op, false),
        };
        let code = match op {
            "add" | "sub" => 0,
            "and" => 1,
            "or" => 2,
            "xor" => 3,
            "smin" => 4,
            "smax" => 5,
            "umin" => 6,
            "umax" => 7,
            "swap" => 8,
            "cmpswap" => u32::MAX,
            _ => return Err(name),
        };
        let values = if code == u32::MAX { 2 } else { 1 } * if wide { 2 } else { 1 };
        let vals = self.vgprs(inst, d, values)?;
        let returns = match dst {
            Some(Operand::Vector { reg, .. }) => Some(*reg),
            _ => None,
        };
        let (ty, value, compare) = if wide {
            self.s.flags |= flags::INT64;
            if let Some(&(slot, _)) = self.s.root_uavs.iter().find(|r| r.1 == h)
                && !self.s.atomic64_uavs.contains(&slot)
            {
                self.s.atomic64_uavs.push(slot);
            }
            let held = self.join64(vals[0], vals[1]);
            let compare = (code == u32::MAX).then(|| self.join64(vals[2], vals[3]));
            (self.s.m.int(64), held, compare)
        } else {
            (i32t, vals[0], (code == u32::MAX).then(|| vals[1]))
        };
        let value = if op == "sub" {
            let z = if wide { self.s.m.int_const(ty, 0) } else { self.s.m.i32(0) };
            self.s.m.bin(c, Bin::Sub, z, value)
        } else {
            value
        };
        let sfx = if wide { "i64" } else { "i32" };
        // Lanes that do not run it keep their old value (written only with glc).
        let outside = if wide { self.s.m.int_const(ty, 0) } else { self.s.m.i32(0) };
        let at = self.begin_guarded(guard);
        let r = match compare {
            Some(compare) => {
                let args = [h, off, iu, iu, compare, value];
                self.s.op(
                    self.cur,
                    &format!("atomicCompareExchange.{sfx}"),
                    Attr::NoUnwind,
                    ty,
                    79,
                    &args,
                )
            }
            None => {
                let args = [h, self.s.m.i32(code), off, iu, iu, value];
                self.s.op(
                    self.cur,
                    &format!("atomicBinOp.{sfx}"),
                    Attr::NoUnwind,
                    ty,
                    78,
                    &args,
                )
            }
        };
        let r = self.end_guarded(at, &[(r, outside)])[0];
        if let Some(reg) = returns {
            if wide {
                let (lo, hi) = self.split64(r);
                self.write(inst, Reg::V(reg), lo, guard);
                self.write(inst, Reg::V(reg + 1), hi, guard);
            } else {
                self.write(inst, Reg::V(reg), r, guard);
            }
        }
        Ok(())
    }
}
