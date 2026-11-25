//! Global stores and atomics at an address in a root UAV (as the console
//! BVH builder's shaders write), on the host GPU: one group of 64 threads
//! whose root parameter 0 is a UAV whose 64-bit address arrives in
//! s[0:1]; its contents are read back.

use super::host::{Dispatch, Host, Root};
use super::split_tests::{S_ENDPGM, V_ADD_NC_U32, V_AND_B32, V_MOV_B32, k, try_lower, vgpr, vop1, vop2, vop3_3};
use super::valu_tests::ONE_AT_A_TIME;
use crate::native::{ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource};

const GLOBAL_STORE_BYTE: u8 = 0x18;
const GLOBAL_STORE_DWORD: u8 = 0x1C;
const GLOBAL_ATOMIC_ADD: u8 = 0x32;
const GLOBAL_ATOMIC_SUB_X2: u8 = 0x53;
const GLOBAL_ATOMIC_UMAX_X2: u8 = 0x58;
const V_LSHLREV_B32: u8 = 0x1A;
const V_MAD_U32_U24: u16 = 0x143;
const LITERAL: u16 = 255;

/// GLOBAL `op` at s[0:1] + v[vaddr] with v[data] (`vdst` the old value
/// with glc).
const fn global(op: u8, vdst: u8, vaddr: u8, data: u8, glc: bool) -> [u32; 2] {
    [
        0x37 << 26 | (op as u32) << 18 | (glc as u32) << 16 | 2 << 14,
        (vdst as u32) << 24 | (data as u32) << 8 | vaddr as u32,
    ]
}

/// A wave32 compute program of one 64-thread group (v0: thread id);
/// root parameter 0, a UAV, in s[0:1].
fn program(body: &[u32]) -> NativeProgram {
    let mut code = body.to_vec();
    code.push(S_ENDPGM);
    NativeProgram {
        stage: HwStage::Cs,
        wave64: false,
        vgprs: 16,
        layout: vec![RootParam {
            root: RootType::Descriptor,
            table_offset_dwords: 0,
            sgprs: 2,
            base_register: 0,
            user_data: 0,
            slot: 0,
        }],
        used: vec![UsedResource {
            kind: ResourceType::Uav,
            root: RootType::Descriptor,
            table_offset_dwords: 0,
            lower_bound: 0,
            stride: 0,
            dwords: 0,
            user_data: 0,
            slot: 0,
        }],
        io: StageIo::Cs(ComputeIo {
            rsrc2: 2 << 1,
            threads: [64, 1, 1],
        }),
        code: code.iter().flat_map(|w| w.to_le_bytes()).collect(),
    }
}

/// The root UAV's 160 dwords after the program ran on the hardware GPU.
fn run(body: &[u32]) -> Option<Vec<u32>> {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; skipped");
        return None;
    };
    let shader = try_lower(&program(body), false).unwrap_or_else(|e| panic!("lower: {e}"));
    assert_eq!(shader.root_uavs, [0]);
    let (root, pso) = host
        .compute_pipeline(&shader)
        .unwrap_or_else(|e| panic!("{}: compute_pipeline: {e}", host.adapter()));
    let ud = vec![0; shader.user_data_dwords().max(1) as usize];
    let out = host
        .dispatch(
            &root,
            &pso,
            &Dispatch {
                roots: vec![(0, Root::Constants(ud)), (1, Root::Uav(160))],
                views: &[],
                samplers: &[],
                groups: [1, 1, 1],
            },
        )
        .unwrap_or_else(|e| panic!("{}: dispatch: {e}", host.adapter()));
    Some(out[0].clone())
}

#[test]
fn global_stores_and_atomics_in_a_root_uav() {
    // dword 0: every thread adds 1, returning the old value, which it
    // stores at dword 1 + t; dword 65 + t: 7t.
    let mut body = vec![
        vop2(V_LSHLREV_B32, 1, k(2), 0), // v1 = 4t
        vop1(V_MOV_B32, 3, k(1)),
        vop1(V_MOV_B32, 4, k(0)),
    ];
    body.extend(global(GLOBAL_ATOMIC_ADD, 5, 4, 3, true));
    body.push(vop2(V_ADD_NC_U32, 6, k(4), 1));
    body.extend(global(GLOBAL_STORE_DWORD, 0, 6, 5, false));
    body.extend(vop3_3(V_MAD_U32_U24, 7, vgpr(0), k(7), k(0)));
    body.extend([vop2(V_ADD_NC_U32, 8, LITERAL, 1), 260]);
    body.extend(global(GLOBAL_STORE_DWORD, 0, 8, 7, false));
    let Some(out) = run(&body) else { return };
    assert_eq!(out[0], 64, "the counter");
    let mut olds = out[1..65].to_vec();
    olds.sort_unstable();
    assert_eq!(olds, (0..64).collect::<Vec<u32>>(), "each old value once");
    assert_eq!(out[65..129], (0..64).map(|t| 7 * t).collect::<Vec<u32>>()[..]);
}

#[test]
fn wide_atomics_and_bytes_in_a_root_uav() {
    // Bytes 0..63: 64-bit umax per slot t & 7 of (100 + t, t); bytes
    // 64..127: 64-bit 0 − 8; bytes 128..191: byte t = 3t + 1.
    let mut body = vec![
        vop2(V_AND_B32, 1, k(7), 0),
        vop2(V_LSHLREV_B32, 2, k(3), 1), // v2 = 8 (t & 7)
        vop2(V_ADD_NC_U32, 3, LITERAL, 0),
        100,
        vop1(V_MOV_B32, 4, vgpr(0)),
    ];
    body.extend(global(GLOBAL_ATOMIC_UMAX_X2, 0, 2, 3, false));
    body.extend([
        vop2(V_ADD_NC_U32, 5, k(64), 2),
        vop1(V_MOV_B32, 6, k(1)),
        vop1(V_MOV_B32, 7, k(0)),
    ]);
    body.extend(global(GLOBAL_ATOMIC_SUB_X2, 0, 5, 6, false));
    body.extend(vop3_3(V_MAD_U32_U24, 8, vgpr(0), k(3), k(1)));
    body.extend([vop2(V_ADD_NC_U32, 9, LITERAL, 0), 128]);
    body.extend(global(GLOBAL_STORE_BYTE, 0, 9, 8, false));
    let Some(out) = run(&body) else { return };
    for x in 0..8 {
        assert_eq!((out[2 * x], out[2 * x + 1]), (156 + x as u32, 56 + x as u32), "umax slot {x}");
        assert_eq!((out[16 + 2 * x], out[17 + 2 * x]), (0xFFFF_FFF8, u32::MAX), "sub slot {x}");
    }
    for t in 0..64u32 {
        let byte = out[32 + (t / 4) as usize] >> (8 * (t % 4)) & 0xFF;
        assert_eq!(byte, (3 * t + 1) & 0xFF, "byte {t}");
    }
}
