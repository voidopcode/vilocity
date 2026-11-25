//! Wave64 compute programs on the host GPU in both forms it supports: one
//! 64-lane host wave per console wave (`WaveSize(64)`, AMD), and the split
//! form, two 32-lane host waves of one group ([`super::lower::Parts::split`],
//! the only one on NVIDIA). Each program runs one group of 64 console threads
//! that read across the two halves, and every thread stores one dword at
//! `4 * thread id`.

use super::host::{Dispatch, Host, Root, View};
use super::valu_tests::ONE_AT_A_TIME;
use super::{HostShader, lower, validator};
use crate::native::{
    ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource,
};
use crate::{io, ir, resource, structure};

pub(super) const S_ENDPGM: u32 = 0xBF81_0000;
const VCC: u8 = 106;
const EXEC: u8 = 126;
/// Inline constants: integers 0..=64 are 128 + n, -1 is 193.
pub(super) const fn k(n: u8) -> u16 {
    128 + n as u16
}
pub(super) const MINUS_ONE: u16 = 193;

pub(super) const fn vgpr(n: u8) -> u16 {
    256 + n as u16
}

/// SOP1 `op sdst, src0`.
pub(super) const fn sop1(op: u8, sdst: u8, src0: u8) -> u32 {
    0xBE80_0000 | (sdst as u32) << 16 | (op as u32) << 8 | src0 as u32
}

/// SOP2 `op sdst, src0, src1`.
pub(super) const fn sop2(op: u8, sdst: u8, src0: u8, src1: u8) -> u32 {
    0x8000_0000 | (op as u32) << 23 | (sdst as u32) << 16 | (src1 as u32) << 8 | src0 as u32
}

/// SOPP `op simm16`.
pub(super) const fn sopp(op: u8, simm16: u16) -> u32 {
    0xBF80_0000 | (op as u32) << 16 | simm16 as u32
}

/// VOP1 `op vdst, src0`.
pub(super) const fn vop1(op: u16, vdst: u8, src0: u16) -> u32 {
    0x7E00_0000 | (vdst as u32) << 17 | (op as u32) << 9 | (src0 as u32 & 0x1FF)
}

/// VOP2 `op vdst, src0, vsrc1`.
pub(super) const fn vop2(op: u8, vdst: u8, src0: u16, vsrc1: u8) -> u32 {
    (op as u32) << 25 | (vdst as u32) << 17 | (vsrc1 as u32) << 9 | (src0 as u32 & 0x1FF)
}

/// VOPC `op vcc, src0, vsrc1`.
pub(super) const fn vopc(op: u16, src0: u16, vsrc1: u8) -> u32 {
    0x7C00_0000 | (op as u32) << 17 | (vsrc1 as u32) << 9 | (src0 as u32 & 0x1FF)
}

/// VOP3 `op vdst, src0, src1` (no modifiers; `vdst` is an SGPR for lane reads).
pub(super) const fn vop3(op: u16, vdst: u8, src0: u16, src1: u16) -> [u32; 2] {
    vop3_3(op, vdst, src0, src1, 0)
}

/// VOP3 `op vdst, src0, src1, src2`.
pub(super) const fn vop3_3(op: u16, vdst: u8, src0: u16, src1: u16, src2: u16) -> [u32; 2] {
    [
        0x35 << 26 | (op as u32 & 0x3FF) << 16 | vdst as u32,
        (src0 as u32 & 0x1FF) | (src1 as u32 & 0x1FF) << 9 | (src2 as u32 & 0x1FF) << 18,
    ]
}

/// DS `op vdst, addr, data0` (offsets 0).
const fn ds(op: u8, vdst: u8, addr: u8, data0: u8) -> [u32; 2] {
    [
        0x36 << 26 | (op as u32) << 18,
        (vdst as u32) << 24 | (data0 as u32) << 8 | addr as u32,
    ]
}

pub(super) const S_MOV_B32: u8 = 0x03;
const S_MOV_B64: u8 = 0x04;
const S_AND_B64: u8 = 0x0F;
const S_BRANCH: u8 = 0x02;
const S_WAITCNT: u8 = 0x0C;
const V_READFIRSTLANE_B32: u16 = 0x002;
pub(super) const V_AND_B32: u8 = 0x1B;
pub(super) const V_ADD_NC_U32: u8 = 0x25;
pub(super) const V_SUB_NC_U32: u8 = 0x26;
const V_SUBREV_NC_U32: u8 = 0x27;
const V_CMP_LT_U32: u16 = 0x0C1;
const V_CMP_NE_U32: u16 = 0x0C5;
pub(super) const V_MAD_U32_U24: u16 = 0x143;
const DS_WRITE_B32: u8 = 0x0D;
const DS_READ_B32: u8 = 0x36;
const S_BCNT1_I32_B64: u8 = 0x10;
const S_AND_SAVEEXEC_B64: u8 = 0x24;
const S_OR_B64: u8 = 0x11;
const S_CBRANCH_EXECZ: u8 = 0x08;
pub(super) const V_MOV_B32: u16 = 0x001;
pub(super) const V_LSHLREV_B32: u8 = 0x1A;
const V_CMP_GT_U32: u16 = 0x0C4;
const V_READLANE_B32: u16 = 0x360;
pub(super) const V_MBCNT_LO_U32_B32: u16 = 0x365;
pub(super) const V_MBCNT_HI_U32_B32: u16 = 0x366;

/// A wave64 compute program of one 64-thread group: `body` leaves each
/// thread's result in `v1`, which goes to dword `v0` (the thread id) of the
/// raw UAV at heap index 0.
fn program(body: &[u32]) -> NativeProgram {
    program_in(body, [64, 1, 1], 0)
}

/// [`program`] for a group of `threads` (two dimensions at most: `v0` is
/// then the flat thread id), with `lds` blocks of 512 bytes of LDS.
pub(super) fn program_in(body: &[u32], threads: [u32; 3], lds: u32) -> NativeProgram {
    let two_d = threads[1] > 1;
    let mut w = vec![sop1(S_MOV_B32, 5, k(0) as u8)];
    if two_d {
        // v0 = v1 * width + v0
        w.extend_from_slice(&vop3_3(V_MAD_U32_U24, 0, vgpr(1), k(threads[0] as u8), vgpr(0)));
    }
    // s_load_dwordx4 s[8:11], s[4:5], 0: the UAV's descriptor.
    w.extend_from_slice(&[0x3D << 26 | 0x02 << 18 | 8 << 6 | 4 >> 1, 125 << 25]);
    w.extend_from_slice(body);
    w.push(vop2(V_LSHLREV_B32, 2, k(2), 0));
    // buffer_store_dword v1, v2, s[8:11], 0 offen
    w.extend_from_slice(&[0x38 << 26 | 0x1C << 18 | 1 << 12, 0x80 << 24 | 2 << 16 | 1 << 8 | 2]);
    w.push(S_ENDPGM);
    NativeProgram {
        stage: HwStage::Cs,
        wave64: true,
        vgprs: 8,
        layout: vec![
            RootParam {
                root: RootType::Constants,
                table_offset_dwords: 0,
                sgprs: 4,
                base_register: 0,
                user_data: 0,
                slot: 0,
            },
            RootParam {
                root: RootType::Table,
                table_offset_dwords: 0,
                sgprs: 1,
                base_register: 0,
                user_data: 4,
                slot: 1,
            },
        ],
        used: vec![UsedResource {
            kind: ResourceType::Uav,
            root: RootType::Table,
            table_offset_dwords: 0,
            lower_bound: 0,
            stride: 0,
            dwords: 4,
            user_data: 4,
            slot: 1,
        }],
        io: StageIo::Cs(ComputeIo {
            // 5 user SGPRs: s0..s4; v0 (and v1) = thread id; LDS size.
            rsrc2: 5 << 1 | u32::from(two_d) << 11 | lds << 15,
            threads,
        }),
        code: w.iter().flat_map(|x| x.to_le_bytes()).collect(),
    }
}

fn lower(native: &NativeProgram, split: bool) -> HostShader {
    try_lower(native, split).expect("lower")
}

pub(super) fn try_lower(native: &NativeProgram, split: bool) -> Result<HostShader, String> {
    let p = ir::build(&native.code, native.wave64, native.vgprs).expect("ir::build");
    let st = structure::structure(&p).expect("structure");
    let res = resource::track(&p, &native.layout, native.stage, Some(&st)).expect("track");
    let io = io::raise(&p, native).expect("io::raise");
    let l = lower::lower(&lower::Parts {
        native,
        p: &p,
        st: &st,
        io: &io,
        res: &res,
        inputs: Vec::new(),
        outputs: Vec::new(),
        prims: Vec::new(),
        mesh_stride: 0,
        split,
    })
    .map_err(|e| e.to_string())?;
    let shader = HostShader {
        stage: HwStage::Cs,
        dxil: l.dxil,
        user_data: l.user_data,
        samplers: l.samplers,
        static_samplers: Vec::new(),
        root_cbvs: l.root_cbvs,
        root_srvs: l.root_srvs,
        root_uavs: l.root_uavs,
        root_indexed_views: Vec::new(),
    };
    if let Ok(mut v) = validator::Validator::load(validator::Validator::default_path().as_deref()) {
        let mut copy = shader.dxil.clone();
        v.validate(&mut copy).expect("DXIL validation failed");
    }
    Ok(shader)
}

/// Runs `body` on the hardware GPU in every form it supports and checks
/// each thread's dword against `want(thread)`. WARP is left out: it runs
/// no fixed wave size.
fn check(body: &[u32], want: impl Fn(u32) -> u32) {
    check_in(&program(body), want);
}

/// [`check`] of a whole program; threads past the group's keep 0.
pub(super) fn check_in(native: &NativeProgram, want: impl Fn(u32) -> u32) {
    let StageIo::Cs(cs) = native.io else {
        unreachable!()
    };
    let n: u32 = cs.threads.iter().product();
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; skipped");
        return;
    };
    let (min, max) = host.wave_lanes().expect("wave sizes");
    let mut ran = 0;
    for split in [false, true] {
        let supported = if split { min <= 32 && 32 <= max } else { max >= 64 };
        if !supported {
            continue;
        }
        let form = if split { "split" } else { "wave64" };
        let label = format!("{} ({min}..{max} lanes), {form}", host.adapter());
        let shader = lower(native, split);
        let (root, pso) = host
            .compute_pipeline(&shader)
            .unwrap_or_else(|e| panic!("{label}: compute_pipeline failed: {e}"));
        let mut ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
        for u in &shader.user_data {
            ud[u.index as usize] = match (u.root, u.slot) {
                (RootType::Constants, 0) => 0,
                (RootType::Table, 1) => 0,
                other => panic!("unexpected user data {other:?}"),
            };
        }
        let out = host
            .dispatch(
                &root,
                &pso,
                &Dispatch {
                    roots: vec![(0, Root::Constants(ud))],
                    views: &[View::RawUav(n.max(64))],
                    samplers: &[],
                    groups: [1, 1, 1],
                },
            )
            .unwrap_or_else(|e| panic!("{label}: dispatch failed: {e}"));
        let len = n.max(64);
        let expected: Vec<u32> = (0..len).map(|t| if t < n { want(t) } else { 0 }).collect();
        assert_eq!(out[0][..len as usize], expected[..], "{label}");
        ran += 1;
    }
    assert!(ran > 0, "the GPU runs neither 64- nor 32-lane waves ({min}..{max})");
}

#[test]
fn ballot_counts_lanes_of_both_halves() {
    // v_cmp_gt_u32 vcc, 40, v0; s_bcnt1_i32_b64 s20, vcc; v_mov_b32 v1, s20
    check(
        &[
            vopc(V_CMP_GT_U32, k(40), 0),
            sop1(S_BCNT1_I32_B64, 20, VCC),
            vop1(V_MOV_B32, 1, 20),
        ],
        |_| 40,
    );
}

#[test]
fn readlane_reads_the_other_half() {
    // v_readlane_b32 s20, v0, 40; v_mov_b32 v1, s20
    let [a, b] = vop3(V_READLANE_B32, 20, vgpr(0), k(40));
    check(&[a, b, vop1(V_MOV_B32, 1, 20)], |_| 40);
}

#[test]
fn mbcnt_numbers_the_console_lanes() {
    // v_mbcnt_lo_u32_b32 v1, -1, 0; v_mbcnt_hi_u32_b32 v1, -1, v1
    let [a, b] = vop3(V_MBCNT_LO_U32_B32, 1, MINUS_ONE, k(0));
    let [c, d] = vop3(V_MBCNT_HI_U32_B32, 1, MINUS_ONE, vgpr(1));
    check(&[a, b, c, d], |t| t);
}

/// An `s_cbranch_execz` around code that counts EXEC: lanes below `n` run it.
fn region_below(n: u8) -> Vec<u32> {
    vec![
        vop1(V_MOV_B32, 1, k(7)),
        vopc(V_CMP_GT_U32, k(n), 0),
        sop1(S_AND_SAVEEXEC_B64, 12, VCC),
        sopp(S_CBRANCH_EXECZ, 2),
        sop1(S_BCNT1_I32_B64, 20, EXEC),
        vop1(V_MOV_B32, 1, 20),
        sop2(S_OR_B64, EXEC, EXEC, 12),
    ]
}

#[test]
fn divergent_region_counts_its_lanes_in_both_halves() {
    check(&region_below(40), |t| if t < 40 { 40 } else { 7 });
    check(&region_below(0), |_| 7);
    check(&region_below(64), |_| 64);
}

#[test]
fn loop_with_lanes_leaving_counts_each_iteration() {
    // Lane t loops t & 3 times; each iteration adds the lanes still in it
    // (48, 32, 16): the wave keeps looping while any lane of either half does.
    check(
        &[
            vop1(V_MOV_B32, 1, k(0)),
            vop2(V_AND_B32, 3, k(3), 0),
            sop1(S_MOV_B64, 12, EXEC),
            // 3: loop
            vopc(V_CMP_NE_U32, k(0), 3),
            sop2(S_AND_B64, EXEC, EXEC, VCC),
            sopp(S_CBRANCH_EXECZ, 4), // -> 10
            sop1(S_BCNT1_I32_B64, 20, EXEC),
            vop2(V_ADD_NC_U32, 1, 20, 1),
            vop2(V_SUBREV_NC_U32, 3, k(1), 3),
            sopp(S_BRANCH, -7i16 as u16), // -> 3
            // 10:
            sop1(S_MOV_B64, EXEC, 12),
        ],
        |t| [0, 48, 80, 96][(t & 3) as usize],
    );
}

/// `v_readfirstlane` of the thread id where EXEC holds lanes above `n`.
fn first_above(n: u8) -> Vec<u32> {
    vec![
        vop1(V_MOV_B32, 1, k(7)),
        vopc(V_CMP_LT_U32, k(n), 0),
        sop1(S_AND_SAVEEXEC_B64, 12, VCC),
        vop1(V_READFIRSTLANE_B32, 20, vgpr(0)),
        vop1(V_MOV_B32, 1, 20),
        sop2(S_OR_B64, EXEC, EXEC, 12),
    ]
}

#[test]
fn readfirstlane_takes_the_first_lane_in_exec() {
    check(&first_above(5), |t| if t > 5 { 6 } else { 7 });
    check(&first_above(39), |t| if t > 39 { 40 } else { 7 });
}

#[test]
fn lds_written_by_one_half_is_read_by_the_other() {
    // ds_write lds[t] = t; ds_read v1 = lds[63 - t]: the console's single
    // wave needs no barrier for this, two host waves do.
    let mut body = vec![vop2(V_LSHLREV_B32, 2, k(2), 0)];
    body.extend_from_slice(&ds(DS_WRITE_B32, 0, 2, 0));
    body.push(sopp(S_WAITCNT, 0));
    body.push(vop2(V_SUB_NC_U32, 4, k(63), 0));
    body.push(vop2(V_LSHLREV_B32, 4, k(2), 4));
    body.extend_from_slice(&ds(DS_READ_B32, 1, 4, 0));
    body.push(sopp(S_WAITCNT, 0));
    check_in(&program_in(&body, [64, 1, 1], 1), |t| 63 - t);
}

const BALLOT_40: [u32; 3] = [
    vopc(V_CMP_GT_U32, k(40), 0),
    sop1(S_BCNT1_I32_B64, 20, VCC),
    vop1(V_MOV_B32, 1, 20),
];

#[test]
fn partial_upper_half_and_two_d_groups() {
    check_in(&program_in(&BALLOT_40, [48, 1, 1], 0), |_| 40);
    check_in(&program_in(&BALLOT_40, [8, 8, 1], 0), |_| 40);
    let [a, b] = vop3(V_MBCNT_LO_U32_B32, 1, MINUS_ONE, k(0));
    let [c, d] = vop3(V_MBCNT_HI_U32_B32, 1, MINUS_ONE, vgpr(1));
    check_in(&program_in(&[a, b, c, d], [8, 8, 1], 0), |t| t);
}

pub(super) const V_LSHRREV_B32: u8 = 0x16;
const VCC_HI: u8 = 107;

/// `s_bcnt1` of the lanes of each console wave below lane 40 of it.
fn ballot_per_wave() -> Vec<u32> {
    vec![
        vop2(V_AND_B32, 5, k(63), 0),
        vopc(V_CMP_GT_U32, k(40), 5),
        sop1(S_BCNT1_I32_B64, 20, VCC),
        vop1(V_MOV_B32, 1, 20),
    ]
}

#[test]
fn groups_of_several_console_waves_meet_in_pairs() {
    // 96 threads: the second console wave has no upper half.
    check_in(&program_in(&ballot_per_wave(), [96, 1, 1], 0), |t| if t < 64 { 40 } else { 32 });
    check_in(&program_in(&ballot_per_wave(), [128, 1, 1], 0), |_| 40);
    check_in(&program_in(&ballot_per_wave(), [256, 1, 1], 0), |_| 40);
    check_in(&program_in(&ballot_per_wave(), [16, 8, 1], 0), |_| 40);
    // v_readlane_b32 s20, v0, 40: lane 40 of the thread's own console wave.
    let [a, b] = vop3(V_READLANE_B32, 20, vgpr(0), k(40));
    check_in(&program_in(&[a, b, vop1(V_MOV_B32, 1, 20)], [128, 1, 1], 0), |t| t / 64 * 64 + 40);
}

#[test]
fn mbcnt_of_a_mask_in_several_console_waves() {
    // Lanes below 40 of the group (all in the first console wave) counted
    // below each lane: the mask read as a number inside a vector instruction.
    let [a, b] = vop3(V_MBCNT_LO_U32_B32, 1, VCC as u16, k(0));
    let [c, d] = vop3(V_MBCNT_HI_U32_B32, 1, VCC_HI as u16, vgpr(1));
    let body = [vopc(V_CMP_GT_U32, k(40), 0), a, b, c, d];
    check_in(&program_in(&body, [128, 1, 1], 0), |t| if t < 64 { t.min(40) } else { 0 });
}

#[test]
fn console_waves_loop_different_counts() {
    // Lane t of console wave w loops w + (t & 1) times; each iteration adds
    // the lanes of its wave still in the loop. The waves leave the loop at
    // different times, so a group barrier inside it would hang.
    let body = [
        vop1(V_MOV_B32, 1, k(0)),
        vop2(V_LSHRREV_B32, 3, k(6), 0),
        vop2(V_AND_B32, 4, k(1), 0),
        vop2(V_ADD_NC_U32, 3, vgpr(4), 3),
        sop1(S_MOV_B64, 12, EXEC),
        // 5: loop
        vopc(V_CMP_NE_U32, k(0), 3),
        sop2(S_AND_B64, EXEC, EXEC, VCC),
        sopp(S_CBRANCH_EXECZ, 4), // -> 12
        sop1(S_BCNT1_I32_B64, 20, EXEC),
        vop2(V_ADD_NC_U32, 1, 20, 1),
        vop2(V_SUBREV_NC_U32, 3, k(1), 3),
        sopp(S_BRANCH, -7i16 as u16), // -> 5
        // 12:
        sop1(S_MOV_B64, EXEC, 12),
    ];
    check_in(&program_in(&body, [256, 1, 1], 0), |t| {
        let w = t / 64;
        64 * w + if t & 1 != 0 { 32 } else { 0 }
    });
}

#[test]
fn a_half_waits_for_its_late_partner() {
    // The upper halves dawdle before every exchange: the lower ones must
    // wait for their words. (Without the wait this reads stale slots.)
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            lower::DELAY_UPPER.store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let _reset = Reset;
    lower::DELAY_UPPER.store(true, std::sync::atomic::Ordering::Relaxed);
    check_in(&program_in(&ballot_per_wave(), [128, 1, 1], 0), |_| 40);
    let [a, b] = vop3(V_READLANE_B32, 20, vgpr(0), k(40));
    check_in(&program_in(&[a, b, vop1(V_MOV_B32, 1, 20)], [256, 1, 1], 0), |t| t / 64 * 64 + 40);
}

