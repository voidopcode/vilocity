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
        half_views: l.half_views,
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



/// The console's whole-wave minimum (DPP8 swaps, a row mirror,
/// `v_permlanex16`, then lane 32 and lane 0 read back) as
/// `CS_ShadowUnifiedClearRaster` takes it over a 16x16 tile's depth: inside
/// `s_or_saveexec -1`, lanes out of EXEC give +inf. Lanes 20-63 of each
/// console wave are in EXEC with 100 - lane, so the minimum is 37 (lane 63,
/// upper half) in every wave of the 256-thread group.
#[test]
fn whole_wave_minimum_of_a_tile_reads_both_halves() {
    const S_OR_SAVEEXEC_B64: u8 = 0x25;
    const V_CVT_F32_U32: u16 = 0x06;
    const V_SUB_F32: u8 = 0x04;
    const V_MIN_F32: u8 = 0x0F;
    const V_CNDMASK_B32_E64: u16 = 0x101;
    const V_PERMLANEX16_B32: u16 = 0x378;
    const LITERAL: u16 = 0xFF;
    let dpp8 = |lanes: [u32; 8]| -> [u32; 2] {
        let sel = lanes.iter().enumerate().fold(0, |s, (i, l)| s | l << (3 * i));
        [vop2(V_MIN_F32, 2, 0xE9, 2), 2 | sel << 8]
    };
    let mut body = vec![
        vop2(V_AND_B32, 5, k(63), 0),
        vop1(V_CVT_F32_U32, 8, vgpr(5)),
        vop2(V_SUB_F32, 8, LITERAL, 8),
        100f32.to_bits(),
        vop1(V_MOV_B32, 1, k(7)),
        vopc(V_CMP_LT_U32, k(19), 5),
        sop1(S_AND_SAVEEXEC_B64, 12, VCC),
        sop1(S_OR_SAVEEXEC_B64, 14, MINUS_ONE as u8),
        vop1(V_MOV_B32, 2, LITERAL),
        f32::INFINITY.to_bits(),
    ];
    body.extend_from_slice(&vop3_3(V_CNDMASK_B32_E64, 2, vgpr(2), vgpr(8), 14));
    body.extend_from_slice(&dpp8([1, 0, 3, 2, 5, 4, 7, 6]));
    body.extend_from_slice(&dpp8([2, 2, 0, 0, 6, 6, 4, 4]));
    body.extend_from_slice(&dpp8([4, 4, 4, 4, 0, 0, 0, 0]));
    // row_mirror bound_ctrl:1
    body.extend_from_slice(&[vop2(V_MIN_F32, 2, 0xFA, 2), 2 | 0x140 << 8 | 1 << 19 | 0xF << 24 | 0xF << 28]);
    body.extend_from_slice(&vop3_3(V_PERMLANEX16_B32, 3, vgpr(2), k(0), k(0)));
    body.push(vop2(V_MIN_F32, 2, vgpr(3), 2));
    body.extend_from_slice(&vop3(V_READLANE_B32, 16, vgpr(2), k(32)));
    body.push(vop2(V_MIN_F32, 2, 16, 2));
    body.extend_from_slice(&vop3(V_READLANE_B32, 16, vgpr(2), k(0)));
    body.push(sop1(S_MOV_B64, EXEC, 14));
    body.push(vop1(V_MOV_B32, 1, 16));
    body.push(sop2(S_OR_B64, EXEC, EXEC, 12));
    let want = |t: u32| if t % 64 >= 20 { 37f32.to_bits() } else { 7 };
    check_in(&program_in(&body, [64, 1, 1], 0), want);
    check_in(&program_in(&body, [16, 16, 1], 0), want);
}

/// `CS_WriteRasterClassificationResults`'s count per console wave in a
/// 1024-thread group: the wave's first lane in EXEC (`s_ff1` of EXEC
/// against `v_mbcnt`) writes the wave's ballot count to LDS at the wave's
/// slot; after a barrier every thread reads its own wave's slot and the
/// next wave's. Every wave must have written (on the host, before the fix,
/// only wave 0's tiles reached the shadow tile lists).
#[test]
fn each_console_wave_of_a_big_group_writes_its_first_lane_slot() {
    const S_FF1_I32_B64: u8 = 0x14;
    const V_CMPX_EQ_I32: u16 = 0x92;
    const S_BARRIER: u8 = 0x0A;
    let [a, b] = vop3(V_MBCNT_LO_U32_B32, 8, MINUS_ONE, k(0));
    let [c, d] = vop3(V_MBCNT_HI_U32_B32, 8, MINUS_ONE, vgpr(8));
    let mut body = vec![a, b, c, d];
    body.extend_from_slice(&[
        // the wave's count: its lanes below 40, plus its number (t / 64)
        vopc(V_CMP_GT_U32, k(40), 8),
        sop1(S_BCNT1_I32_B64, 20, VCC),
        vop2(V_LSHRREV_B32, 6, k(6), 0),
        vop2(V_ADD_NC_U32, 9, 20, 6),
        // slot = 4 * wave
        vop2(V_LSHLREV_B32, 7, k(2), 6),
        sop1(S_FF1_I32_B64, 4, EXEC),
        sop1(S_MOV_B64, 2, EXEC),
        vopc(V_CMPX_EQ_I32, 4, 8),
    ]);
    body.extend_from_slice(&ds(DS_WRITE_B32, 0, 7, 9));
    body.push(sop1(S_MOV_B64, EXEC, 2));
    body.push(sopp(S_WAITCNT, 0));
    body.push(sopp(S_BARRIER, 0));
    // v1 = lds[slot] * 100 + lds[(slot + 4) % 64]
    body.extend_from_slice(&ds(DS_READ_B32, 1, 7, 0));
    body.push(vop2(V_ADD_NC_U32, 10, k(4), 7));
    body.push(vop2(V_AND_B32, 10, k(63), 10));
    body.extend_from_slice(&ds(DS_READ_B32, 11, 10, 0));
    body.push(sopp(S_WAITCNT, 0));
    body.extend_from_slice(&vop3_3(V_MAD_U32_U24, 1, vgpr(1), 0xFF, vgpr(11)));
    body.push(100);
    let wave = |t: u32| 40 + t / 64;
    check_in(&program_in(&body, [1024, 1, 1], 1), |t| wave(t) * 100 + wave((t + 64) % 1024));
}

/// The next step of `CS_WriteRasterClassificationResults`: thread 0 alone
/// turns the 16 waves' counts into offsets (a loop over the LDS slots with
/// `v_readfirstlane` where only it runs), then after a barrier every wave
/// reads its offset: the counts of the waves before it.
#[test]
fn thread_zero_sums_the_waves_counts_for_every_wave() {
    const S_FF1_I32_B64: u8 = 0x14;
    const V_CMPX_EQ_I32: u16 = 0x92;
    const V_CMP_EQ_U32: u16 = 0x0C2;
    const S_BARRIER: u8 = 0x0A;
    const S_LSHL_B32: u8 = 0x1E;
    const S_ADD_I32: u8 = 0x02;
    const S_CBRANCH_SCC0: u8 = 0x04;
    const fn sopc(op: u8, src0: u8, src1: u8) -> u32 {
        0xBF00_0000 | (op as u32) << 16 | (src1 as u32) << 8 | src0 as u32
    }
    const S_CMP_GT_U32: u8 = 0x08;
    let [a, b] = vop3(V_MBCNT_LO_U32_B32, 8, MINUS_ONE, k(0));
    let [c, d] = vop3(V_MBCNT_HI_U32_B32, 8, MINUS_ONE, vgpr(8));
    let mut body = vec![a, b, c, d];
    body.extend_from_slice(&[
        vopc(V_CMP_GT_U32, k(40), 8),
        sop1(S_BCNT1_I32_B64, 20, VCC),
        vop2(V_LSHRREV_B32, 6, k(6), 0),
        vop2(V_ADD_NC_U32, 9, 20, 6),
        vop2(V_LSHLREV_B32, 7, k(2), 6),
        sop1(S_FF1_I32_B64, 26, EXEC),
        sop1(S_MOV_B64, 28, EXEC),
        vopc(V_CMPX_EQ_I32, 26, 8),
    ]);
    body.extend_from_slice(&ds(DS_WRITE_B32, 0, 7, 9));
    body.push(sop1(S_MOV_B64, EXEC, 28));
    body.push(sopp(S_WAITCNT, 0));
    body.push(sopp(S_BARRIER, 0));
    // thread 0: s31 = 0; for slot in 0..16 { v = lds[slot] + s31; lds[slot] = s31; s31 = first(v) }
    body.push(vopc(V_CMP_EQ_U32, k(0), 0));
    body.push(sop1(S_AND_SAVEEXEC_B64, 24, VCC));
    let skip_at = body.len();
    body.push(0); // s_cbranch_execz -> end
    body.push(sop1(S_MOV_B32, 30, k(0) as u8));
    body.push(sop1(S_MOV_B32, 31, k(0) as u8));
    let top = body.len();
    body.push(sop2(S_LSHL_B32, 16, 30, k(2) as u8));
    body.push(sop2(S_ADD_I32, 30, 30, k(1) as u8));
    body.push(sopc(S_CMP_GT_U32, 30, k(15) as u8));
    body.push(vop1(V_MOV_B32, 37, 16));
    body.extend_from_slice(&ds(DS_READ_B32, 38, 37, 0));
    body.push(sopp(S_WAITCNT, 0));
    body.push(vop2(V_ADD_NC_U32, 38, 31, 38));
    body.push(vop1(V_READFIRSTLANE_B32, 16, vgpr(38)));
    body.push(vop1(V_MOV_B32, 38, 31));
    body.extend_from_slice(&ds(DS_WRITE_B32, 0, 37, 38));
    body.push(sop1(S_MOV_B32, 31, 16));
    let back = body.len();
    body.push(sopp(S_CBRANCH_SCC0, (top as i32 - back as i32 - 1) as i16 as u16));
    let end = body.len();
    body[skip_at] = sopp(S_CBRANCH_EXECZ, (end - skip_at - 1) as u16);
    body.push(sop1(S_MOV_B64, EXEC, 24));
    body.push(sopp(S_WAITCNT, 0));
    body.push(sopp(S_BARRIER, 0));
    body.extend_from_slice(&ds(DS_READ_B32, 1, 7, 0));
    body.push(sopp(S_WAITCNT, 0));
    let before = |w: u32| (0..w).map(|u| 40 + u).sum::<u32>();
    check_in(&program_in(&body, [1024, 1, 1], 1), |t| before(t / 64));
}

#[test]
fn a_histogram_prefix_sum_by_thread_0_reaches_every_thread() {
    // ComputeAverageDepthWeightedLuminanceFromHistogramCS (GTA VI's exposure): bin t's count
    // (here t & 3) in lds[t]; thread 0 alone writes the counts before each bin to lds[256 + t]
    // in a loop it enters in the middle (the console's words from 0x104 to 0x5f0); every
    // thread reads its own after a barrier.
    let mut body = vec![
        0x3408_0082, // v_lshlrev_b32 v4, 2, v0
        0x3602_0083, // v_and_b32 v1, 3, v0
        0xBE94_0380, // s_mov_b32 s20, 0
        0x7D84_0080, // v_cmp_eq_u32 vcc, 0, v0
        0xD834_0000, 0x0000_0104, // ds_write_b32 v4, v1
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0xBF8A_0000, // s_barrier
    ];
    body.extend_from_slice(&[
        0xBE80_246A, // s_and_saveexec_b64 s[0:1], vcc
        0xBF88_013A, // s_cbranch_execz label_05F4
        0xBE8E_0380, // s_mov_b32 s14, 0
        0xBF82_0003, // s_branch label_0120
        0xBF80_0000, // (0x114, past the branch: a word the listing leaves out)
        0x7E1C_0505, // v_readfirstlane_b32 s14, v5
        0xBF80_0000, // s_nop 0x0000
        0x8F0C_8214, // s_lshl_b32 s12, s20, 2
        0x7E16_020E, // v_mov_b32 v11, s14
        0x7E12_020C, // v_mov_b32 v9, s12
        0x810D_FF0C, 0x0000_0400, // s_add_i32 s13, s12, lit(0x00000400)
        0x8114_C014, // s_add_i32 s20, s20, 64
        0x7E14_020D, // v_mov_b32 v10, s13
        0x810D_FF0C, 0x0000_0408, // s_add_i32 s13, s12, lit(0x00000408)
        0xD8DC_0302, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:2 offset1:3
        0xD8DC_0100, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset1:1
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_0E0E, // v_add_nc_u32 v13, s14, v7
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0410, // s_add_i32 s13, s12, lit(0x00000410)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0B0A, // ds_write2_b32 v10, v11, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0418, // s_add_i32 s13, s12, lit(0x00000418)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_0706, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:6 offset1:7
        0xD8DC_0504, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:4 offset1:5
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0420, // s_add_i32 s13, s12, lit(0x00000420)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0428, // s_add_i32 s13, s12, lit(0x00000428)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_0B0A, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:10 offset1:11
        0xD8DC_0908, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:8 offset1:9
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0430, // s_add_i32 s13, s12, lit(0x00000430)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0438, // s_add_i32 s13, s12, lit(0x00000438)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_0F0E, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:14 offset1:15
        0xD8DC_0D0C, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:12 offset1:13
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0440, // s_add_i32 s13, s12, lit(0x00000440)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0448, // s_add_i32 s13, s12, lit(0x00000448)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_1312, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:18 offset1:19
        0xD8DC_1110, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:16 offset1:17
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0450, // s_add_i32 s13, s12, lit(0x00000450)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0458, // s_add_i32 s13, s12, lit(0x00000458)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_1716, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:22 offset1:23
        0xD8DC_1514, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:20 offset1:21
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0460, // s_add_i32 s13, s12, lit(0x00000460)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0468, // s_add_i32 s13, s12, lit(0x00000468)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_1B1A, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:26 offset1:27
        0xD8DC_1918, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:24 offset1:25
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0470, // s_add_i32 s13, s12, lit(0x00000470)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0478, // s_add_i32 s13, s12, lit(0x00000478)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_1F1E, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:30 offset1:31
        0xD8DC_1D1C, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:28 offset1:29
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0480, // s_add_i32 s13, s12, lit(0x00000480)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0488, // s_add_i32 s13, s12, lit(0x00000488)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_2322, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:34 offset1:35
        0xD8DC_2120, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:32 offset1:33
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_0490, // s_add_i32 s13, s12, lit(0x00000490)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_0498, // s_add_i32 s13, s12, lit(0x00000498)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_2726, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:38 offset1:39
        0xD8DC_2524, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:36 offset1:37
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_04A0, // s_add_i32 s13, s12, lit(0x000004a0)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_04A8, // s_add_i32 s13, s12, lit(0x000004a8)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_2B2A, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:42 offset1:43
        0xD8DC_2928, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:40 offset1:41
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_04B0, // s_add_i32 s13, s12, lit(0x000004b0)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_04B8, // s_add_i32 s13, s12, lit(0x000004b8)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_2F2E, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:46 offset1:47
        0xD8DC_2D2C, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:44 offset1:45
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_04C0, // s_add_i32 s13, s12, lit(0x000004c0)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_04C8, // s_add_i32 s13, s12, lit(0x000004c8)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_3332, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:50 offset1:51
        0xD8DC_3130, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:48 offset1:49
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_04D0, // s_add_i32 s13, s12, lit(0x000004d0)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_04D8, // s_add_i32 s13, s12, lit(0x000004d8)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_3736, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:54 offset1:55
        0xD8DC_3534, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:52 offset1:53
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_04E0, // s_add_i32 s13, s12, lit(0x000004e0)
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x7E16_020D, // v_mov_b32 v11, s13
        0x810D_FF0C, 0x0000_04E8, // s_add_i32 s13, s12, lit(0x000004e8)
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_3B3A, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:58 offset1:59
        0xD8DC_3938, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:56 offset1:57
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A1A_1507, // v_add_nc_u32 v13, v7, v10
        0x7E0E_020D, // v_mov_b32 v7, s13
        0x810D_FF0C, 0x0000_04F0, // s_add_i32 s13, s12, lit(0x000004f0)
        0xB78C_04F8, // s_addk_i32 s12, 0x04f8
        0xB194_0100, // s_cmpk_eq_i32 s20, 0x0100
        0x4A10_1B08, // v_add_nc_u32 v8, v8, v13
        0xD838_0100, 0x000D_0A0B, // ds_write2_b32 v11, v10, v13 offset1:1
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A14_0B06, // v_add_nc_u32 v10, v6, v5
        0xD8DC_3F3E, 0x0500_0009, // ds_read2_b32 v[5:6], v9 offset0:62 offset1:63
        0xD8DC_3D3C, 0x0700_0009, // ds_read2_b32 v[7:8], v9 offset0:60 offset1:61
        0x7E12_020D, // v_mov_b32 v9, s13
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0x4A0E_1507, // v_add_nc_u32 v7, v7, v10
        0x4A10_0F08, // v_add_nc_u32 v8, v8, v7
        0xD838_0100, 0x0007_0A09, // ds_write2_b32 v9, v10, v7 offset1:1
        0x7E0E_020C, // v_mov_b32 v7, s12
        0x4A0A_1105, // v_add_nc_u32 v5, v5, v8
        0xD838_0100, 0x0005_0807, // ds_write2_b32 v7, v8, v5 offset1:1
        0x4A0A_0B06, // v_add_nc_u32 v5, v6, v5
        0xBF84_FEC9, // s_cbranch_scc0 label_0118
    ]);
    body.extend_from_slice(&[
        0xBEFE_0400, // s_mov_b64 exec, s[0:1]
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
        0xBF8A_0000, // s_barrier
        0xD8D8_0400, 0x0100_0004, // ds_read_b32 v1, v4 offset:1024
        0xBF8C_C07F, // s_waitcnt lgkmcnt(0)
    ]);
    // Its barriers wait only with the program's name in the list (`super::sync_full`): fenced
    // only, threads 32 and up read zeros (the white daylight of GTA VI).
    let name = b"x ComputeAverageDepthWeightedLuminanceFromHistogramCS_b3fd0161_Wrapped";
    let was = super::SYNC_FULL.replace(super::sync_full(name));
    check_in(&program_in(&body, [256, 1, 1], 4), |t| (0..t).map(|i| i & 3).sum());
    super::SYNC_FULL.set(was);
}

