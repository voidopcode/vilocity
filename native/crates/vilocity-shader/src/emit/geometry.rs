//! Legacy geometry stages (merged ES+GS run as a mesh shader): the
//! emitter's geometry helpers, a child module of `lower` so they share its
//! state.

use super::*;

impl<'a> Lower<'a> {
    /// Geometry ([`StageIo::Gs`]): the merged program's interface.
    pub(super) fn geo(&self) -> Option<&'a GeometryIo> {
        match &self.x.native.io {
            StageIo::Gs(g) => Some(g),
            _ => None,
        }
    }

    /// Geometry: the pointer to bookkeeping entry `base + index`.
    pub(super) fn gs_slot(&mut self, base: u32, index: V) -> V {
        let arr = match self.gs_array {
            Some(a) => a,
            None => {
                let max_p = self.s.mesh.expect("geometry").max_primitives;
                let ty = self.s.m.array(GS_TRIANGLES + max_p + 1, self.s.t.i32);
                let a = self.s.m.global("vilocity_gs", ty, 3);
                self.gs_array = Some(a);
                a
            }
        };
        let k = self.s.m.i32(base);
        let at = self.s.m.bin(self.cur, Bin::Add, index, k);
        self.s.m.element(self.cur, arr, at)
    }

    /// Geometry: dword `byte / 4` of the draw's input ([`GEOMETRY_INPUT`]).
    pub(super) fn gs_input_dword(&mut self, byte: V) -> V {
        let e = self.s.entry;
        let h = self.s.root_srv_handle(GEOMETRY_INPUT);
        let (u, mask, al) = (self.s.m.undef(self.s.t.i32), self.s.m.i8(1), self.s.m.i32(4));
        let r = self.s.op(
            e,
            "rawBufferLoad.f32",
            Attr::ReadOnly,
            self.s.t.res_ret,
            139,
            &[h, byte, u, mask, al],
        );
        let x = self.s.m.extract(e, r, 0);
        self.as_int(x)
    }

    /// Geometry: (group id, triangles in this group, lane), in the entry
    /// block. A group takes [`GeometryIo::PRIMITIVES`] triangles of the draw.
    pub(super) fn gs_group(&mut self) -> (V, V, V) {
        if let Some(g) = self.gs_group {
            return g;
        }
        let e = self.s.entry;
        let Val::I32(gid) = self.compute_id("groupId.i32", 94, 0) else {
            unreachable!()
        };
        let z = self.s.m.i32(0);
        let total = self.gs_input_dword(z);
        let per = self.s.m.i32(GeometryIo::PRIMITIVES);
        let done = self.s.m.bin(e, Bin::Mul, gid, per);
        let left = self.s.m.bin(e, Bin::Sub, total, done);
        let fewer = self.s.m.cmp(e, pred::IULT, left, per);
        let prims = self.s.m.select(e, fewer, left, per);
        let lane = self.mesh_thread();
        self.gs_group = Some((gid, prims, lane));
        (gid, prims, lane)
    }

    /// Geometry: the value of a merged-stage input; `None` for the others.
    pub(super) fn gs_input(&mut self, g: &GeometryIo, input: Input) -> Option<V> {
        let e = self.s.entry;
        let (gid, prims, lane) = self.gs_group();
        let m = |l: &mut Self, k: u32| l.s.m.i32(k);
        let item = g.es_item_bytes / 4;
        let first = |l: &mut Self| {
            let three = m(l, 3 * item);
            l.s.m.bin(e, Bin::Mul, lane, three)
        };
        Some(match input {
            // The triangle of lane p has ES vertices 3p..3p + 2.
            Input::GsOffsets01 => {
                let o0 = first(self);
                let k = m(self, item);
                let o1 = self.s.m.bin(e, Bin::Add, o0, k);
                let k16 = m(self, 16);
                let o1 = self.s.m.bin(e, Bin::Shl, o1, k16);
                self.s.m.bin(e, Bin::Or, o0, o1)
            }
            Input::GsOffset2 => {
                let o0 = first(self);
                let k = m(self, 2 * item);
                self.s.m.bin(e, Bin::Add, o0, k)
            }
            Input::GsOutputBase => m(self, 64 * g.es_item_bytes),
            Input::MergedWaveInfo => {
                let three = m(self, 3);
                let verts = self.s.m.bin(e, Bin::Mul, prims, three);
                let k8 = m(self, 8);
                let p = self.s.m.bin(e, Bin::Shl, prims, k8);
                self.s.m.bin(e, Bin::Or, verts, p)
            }
            Input::PrimitiveId => {
                let per = m(self, GeometryIo::PRIMITIVES);
                let first = self.s.m.bin(e, Bin::Mul, gid, per);
                self.s.m.bin(e, Bin::Add, first, lane)
            }
            Input::EsAttribute { row, channel } => {
                // Lanes past the group's corners read its last one.
                let (three, one) = (m(self, 3), m(self, 1));
                let corners = self.s.m.bin(e, Bin::Mul, prims, three);
                let last = self.s.m.bin(e, Bin::Sub, corners, one);
                let inside = self.s.m.cmp(e, pred::IULT, lane, corners);
                let corner = self.s.m.select(e, inside, lane, last);
                let per = m(self, 3 * GeometryIo::PRIMITIVES);
                let before = self.s.m.bin(e, Bin::Mul, gid, per);
                let corner = self.s.m.bin(e, Bin::Add, before, corner);
                let rows = m(self, g.input_rows());
                let at = self.s.m.bin(e, Bin::Mul, corner, rows);
                let r = m(self, u32::from(row));
                let at = self.s.m.bin(e, Bin::Add, at, r);
                let k16 = m(self, 16);
                let byte = self.s.m.bin(e, Bin::Mul, at, k16);
                let off = m(self, 16 + 4 * u32::from(channel));
                let byte = self.s.m.bin(e, Bin::Add, byte, off);
                self.gs_input_dword(byte)
            }
            _ => return None,
        })
    }

    /// Geometry: the output counts (the maxima, as for mesh programs: the
    /// epilogue culls what is unused) and the bookkeeping cleared, before
    /// any lane runs (a group barrier: every thread is here).
    pub(super) fn gs_prologue(&mut self, mesh: crate::emit::shader::Mesh) -> Result<(), Why> {
        if mesh.max_vertices > 256 || mesh.max_primitives > 256 {
            return Err("a geometry shader emitting more vertices than a mesh group holds");
        }
        let bytes = 4 * (self.s.lds_dwords + GS_TRIANGLES + mesh.max_primitives + 1);
        if bytes > 28 * 1024 {
            return Err("a geometry shader's LDS beyond a mesh shader's");
        }
        let e = self.s.entry;
        let void = self.s.t.void;
        let (v, p) = (
            self.s.m.i32(mesh.max_vertices),
            self.s.m.i32(mesh.max_primitives),
        );
        self.s
            .op(e, "setMeshOutputCounts", Attr::NoUnwind, void, 168, &[v, p]);
        let (_, _, lane) = self.gs_group();
        let (z, all) = (self.s.m.i32(0), self.s.m.i32(u32::MAX));
        for (base, value) in [(GS_VERTICES, z), (GS_STRIP, z), (GS_LOWEST, all)] {
            let p = self.gs_slot(base, lane);
            self.s.m.store(e, p, value);
        }
        // The group's counters: every lane writes the same 0.
        for base in [GS_TOTAL, GS_TRIANGLE_COUNT] {
            let p = self.gs_slot(base, z);
            self.s.m.atomic(e, Rmw::Xchg, p, z);
        }
        self.group_barrier();
        Ok(())
    }

    /// Geometry: `s_sendmsg` of the GS stream 0, in the lanes of EXEC: emit
    /// (the vertex the lane wrote last becomes its next), cut (a new strip),
    /// or both. Each strip vertex from the third on closes a triangle, odd
    /// ones with their first two vertices swapped (the strip's winding).
    pub(super) fn gs_message(&mut self, b: usize, i: usize, imm: u16) -> Result<(), Why> {
        let (msg, op, stream) = (imm & 0xF, imm >> 4 & 3, imm >> 8 & 3);
        if msg != 2 || stream != 0 || op == 0 {
            return Ok(()); // GS done, other streams: nothing drawn
        }
        let on = match self.x.st.execs[b][i] {
            Some(e) => self.boolean(e, self.cur)?,
            None => self.s.m.bool(true),
        };
        let at = self.begin_guarded(Some(on));
        let (_, _, lane) = self.gs_group();
        let one = self.s.m.i32(1);
        if op & 2 != 0 {
            let c = self.cur;
            let pv = self.gs_slot(GS_VERTICES, lane);
            let k = self.s.m.load(c, pv);
            let next = self.s.m.bin(c, Bin::Add, k, one);
            self.s.m.store(c, pv, next);
            let ps = self.gs_slot(GS_STRIP, lane);
            let s = self.s.m.load(c, ps);
            let s = self.s.m.bin(c, Bin::Add, s, one);
            self.s.m.store(c, ps, s);
            let three = self.s.m.i32(3);
            let closes = self.s.m.cmp(c, pred::IUGE, s, three);
            let tri = self.begin_guarded(Some(closes));
            let c = self.cur;
            let z = self.s.m.i32(0);
            let pc = self.gs_slot(GS_TRIANGLE_COUNT, z);
            let t = self.s.m.atomic(c, Rmw::Add, pc, one);
            let j = self.s.m.bin(c, Bin::Sub, s, three);
            let odd = self.s.m.bin(c, Bin::And, j, one);
            let odd = self.s.m.cmp(c, pred::NE, odd, z);
            let two = self.s.m.i32(2);
            let a = self.s.m.bin(c, Bin::Sub, k, two);
            let bb = self.s.m.bin(c, Bin::Sub, k, one);
            let first = self.s.m.select(c, odd, bb, a);
            let second = self.s.m.select(c, odd, a, bb);
            let (k8, k16, k24) = (self.s.m.i32(8), self.s.m.i32(16), self.s.m.i32(24));
            let first = self.s.m.bin(c, Bin::Shl, first, k8);
            let second = self.s.m.bin(c, Bin::Shl, second, k16);
            let third = self.s.m.bin(c, Bin::Shl, k, k24);
            let word = self.s.m.bin(c, Bin::Or, lane, first);
            let word = self.s.m.bin(c, Bin::Or, word, second);
            let word = self.s.m.bin(c, Bin::Or, word, third);
            // More triangles than declared (a program emitting past its
            // count) go to a spare entry.
            let max_p = self.s.mesh.expect("geometry").max_primitives;
            let limit = self.s.m.i32(max_p);
            let fits = self.s.m.cmp(c, pred::IULT, t, limit);
            let t = self.s.m.select(c, fits, t, limit);
            let p = self.gs_slot(GS_TRIANGLES, t);
            self.s.m.store(c, p, word);
            self.end_guarded(tri, &[]);
        }
        if op & 1 != 0 {
            let z = self.s.m.i32(0);
            let ps = self.gs_slot(GS_STRIP, lane);
            self.s.m.store(self.cur, ps, z);
        }
        self.end_guarded(at, &[]);
        Ok(())
    }

    /// Geometry: the block every lane ends in.
    pub(super) fn gs_end_block(&mut self) -> usize {
        match self.gs_end {
            Some(b) => b,
            None => {
                let b = self.s.m.new_block();
                self.gs_end = Some(b);
                b
            }
        }
    }

    /// Geometry: the group's outputs, once every lane has ended. Each lane
    /// takes its vertices' place after the lanes before it (in arrival
    /// order) and copies them from LDS (from its lowest GS byte, a vertex
    /// every `vertex_bytes`); the triangles follow, the rest culled.
    pub(super) fn gs_epilogue(&mut self) -> Result<(), Why> {
        let g = self.geo().expect("geometry");
        let mesh = self.s.mesh.expect("geometry");
        let mvc = g.max_vertex_count.max(1);
        let vb = g.vertex_bytes;
        let (_, _, lane) = self.gs_group();
        self.group_barrier();
        let c = self.cur;
        let pn = self.gs_slot(GS_VERTICES, lane);
        let n = self.s.m.load(c, pn);
        let most = self.s.m.i32(mvc);
        let over = self.s.m.cmp(c, pred::IUGT, n, most);
        let n = self.s.m.select(c, over, most, n);
        let z = self.s.m.i32(0);
        let pt = self.gs_slot(GS_TOTAL, z);
        let base = self.s.m.atomic(c, Rmw::Add, pt, n);
        let pf = self.gs_slot(GS_FIRST, lane);
        self.s.m.store(c, pf, base);
        let pl = self.gs_slot(GS_LOWEST, lane);
        let lowest = self.s.m.load(c, pl);
        self.group_barrier();
        // A lane that wrote nothing: its slot in the output area.
        let c = self.cur;
        let none = self.s.m.i32(u32::MAX);
        let wrote = self.s.m.cmp(c, pred::NE, lowest, none);
        let area = self.s.m.i32(64 * g.es_item_bytes);
        let span = self.s.m.i32(mvc * vb);
        let own = self.s.m.bin(c, Bin::Mul, lane, span);
        let own = self.s.m.bin(c, Bin::Add, area, own);
        let start = self.s.m.select(c, wrote, lowest, own);

        // Vertices: k = 0..n.
        let (head, body, next) = (self.s.m.new_block(), self.s.m.new_block(), self.s.m.new_block());
        self.s.m.terminate(c, Term::Br(head));
        let k = self.s.m.phi(head, self.s.t.i32);
        let more = self.s.m.cmp(head, pred::IULT, k, n);
        self.s.m.terminate(head, Term::CondBr(more, body, next));
        self.cur = body;
        let kv = self.s.m.i32(vb);
        let at = self.s.m.bin(body, Bin::Mul, k, kv);
        let at = self.s.m.bin(body, Bin::Add, start, at);
        let index = self.s.m.bin(body, Bin::Add, base, k);
        let lds = self.s.lds();
        let void = self.s.t.void;
        for id in 0..self.s.outputs.len() {
            let (row, col, cols, float) = {
                let el = &self.s.outputs[id];
                let Some(row) = el.row else { continue };
                (row, el.col, el.cols, el.comp == 9)
            };
            for ch in col..col + cols {
                let byte = self.s.m.i32(16 * row + 4 * u32::from(ch));
                let byte = self.s.m.bin(body, Bin::Add, at, byte);
                let two = self.s.m.i32(2);
                let dword = self.s.m.bin(body, Bin::LShr, byte, two);
                let p = self.s.m.element(body, lds, dword);
                let x = self.s.m.load(body, p);
                let (sfx, x) = if float { (".f32", self.as_float(x)) } else { (".i32", x) };
                let args = [self.s.m.i32(id as u32), self.s.m.i32(0), self.s.m.i8(ch), x, index];
                self.s.op(
                    body,
                    &format!("storeVertexOutput{sfx}"),
                    Attr::NoUnwind,
                    void,
                    171,
                    &args,
                );
                self.s.outputs[id].usage |= 1 << (ch - col);
            }
        }
        let one = self.s.m.i32(1);
        let k1 = self.s.m.bin(body, Bin::Add, k, one);
        self.s.m.terminate(body, Term::Br(head));
        self.s.m.set_phi(k, vec![(z, c), (k1, body)]);

        // Triangles: t = lane, lane + 64, ... below the declared count; past
        // the group's, culled (all indices 0).
        self.cur = next;
        let pc = self.gs_slot(GS_TRIANGLE_COUNT, z);
        let count = self.s.m.load(next, pc);
        let (head, body, done) = (self.s.m.new_block(), self.s.m.new_block(), self.s.m.new_block());
        self.s.m.terminate(next, Term::Br(head));
        let t = self.s.m.phi(head, self.s.t.i32);
        let limit = self.s.m.i32(mesh.max_primitives);
        let more = self.s.m.cmp(head, pred::IULT, t, limit);
        self.s.m.terminate(head, Term::CondBr(more, body, done));
        self.cur = body;
        let real = self.s.m.cmp(body, pred::IULT, t, count);
        let p = self.gs_slot(GS_TRIANGLES, t);
        let word = self.s.m.load(body, p);
        let k63 = self.s.m.i32(63);
        let owner = self.s.m.bin(body, Bin::And, word, k63);
        let pf = self.gs_slot(GS_FIRST, owner);
        let first = self.s.m.load(body, pf);
        let mut args = vec![t];
        for shift in [8, 16, 24] {
            let (sh, ff) = (self.s.m.i32(shift), self.s.m.i32(0xFF));
            let v = self.s.m.bin(body, Bin::LShr, word, sh);
            let v = self.s.m.bin(body, Bin::And, v, ff);
            let v = self.s.m.bin(body, Bin::Add, first, v);
            args.push(self.s.m.select(body, real, v, z));
        }
        self.s
            .op(body, "emitIndices", Attr::NoUnwind, void, 169, &args);
        let k64 = self.s.m.i32(64);
        let t1 = self.s.m.bin(body, Bin::Add, t, k64);
        self.s.m.terminate(body, Term::Br(head));
        self.s.m.set_phi(t, vec![(lane, next), (t1, body)]);
        self.s.m.terminate(done, Term::Ret);
        self.cur = done;
        Ok(())
    }
}
