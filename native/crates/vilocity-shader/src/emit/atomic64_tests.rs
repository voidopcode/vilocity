//! 64-bit image and buffer atomics on the host GPU: 64 console threads each
//! update one of 8 slots (texels of an R32G32_UINT texture, or 8-byte words of a
//! raw buffer) of the UAV at heap index 0, which are read back. Skipped on a
//! GPU without 64-bit atomics on typed resources through the descriptor heap.

use super::host::{Dispatch, Host, Root, View};
use super::split_tests::{
    MINUS_ONE, S_ENDPGM, S_MOV_B32, V_ADD_NC_U32, V_AND_B32, V_MOV_B32, k, sop1, try_lower, vgpr,
    vop1, vop2,
};
use super::valu_tests::ONE_AT_A_TIME;
use crate::native::{
    ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource,
};

const IMAGE_ATOMIC_CMPSWAP: u8 = 0x10;
const IMAGE_ATOMIC_ADD: u8 = 0x11;
const IMAGE_ATOMIC_UMAX: u8 = 0x17;
const LITERAL: u16 = 255;

/// MIMG `op v[vdata], v[vaddr:vaddr+1], s[srsrc:srsrc+7] dmask dim:2D`.
const fn mimg(op: u8, dmask: u8, vdata: u8, vaddr: u8, srsrc: u8) -> [u32; 2] {
    [
        0x3C << 26 | ((op & 0x7F) as u32) << 18 | (dmask as u32) << 8 | 1 << 3 | (op >> 7) as u32,
        vaddr as u32 | (vdata as u32) << 8 | ((srsrc >> 2) as u32) << 16,
    ]
}

/// MUBUF `op v[vdata], v[vaddr], s[srsrc:srsrc+3], 0 offen`.
const fn mubuf(op: u8, vdata: u8, vaddr: u8, srsrc: u8) -> [u32; 2] {
    [
        0x38 << 26 | (op as u32) << 18 | 1 << 12,
        0x80 << 24 | ((srsrc >> 2) as u32) << 16 | (vdata as u32) << 8 | vaddr as u32,
    ]
}

const BUFFER_ATOMIC_SUB: u8 = 0x33;
const BUFFER_ATOMIC_CMPSWAP_X2: u8 = 0x51;
const BUFFER_ATOMIC_ADD_X2: u8 = 0x52;
const BUFFER_ATOMIC_SUB_X2: u8 = 0x53;
const BUFFER_ATOMIC_UMAX_X2: u8 = 0x58;
const V_LSHLREV_B32: u8 = 0x1A;

/// One group of 64 threads: `data` puts the atomic's data in v3.., then the
/// image atomic `op` with `dmask` updates texel (thread & 7, 0).
fn program(data: &[u32], op: u8, dmask: u8) -> NativeProgram {
    let mut w = vec![sop1(S_MOV_B32, 5, k(0) as u8)];
    // s_load_dwordx8 s[8:15], s[4:5], 0: the texture's descriptor.
    w.extend_from_slice(&[0x3D << 26 | 0x03 << 18 | 8 << 6 | 4 >> 1, 125 << 25]);
    w.push(vop2(V_AND_B32, 1, k(7), 0));
    w.push(vop1(V_MOV_B32, 2, k(0)));
    w.extend_from_slice(data);
    w.extend_from_slice(&mimg(op, dmask, 3, 1, 8));
    w.push(S_ENDPGM);
    native(w, 8)
}

/// [`program`] for a raw buffer: the buffer atomic `op` updates the 8-byte
/// word (thread & 7) (or dword, for a 32-bit `op`).
fn buffer_program(data: &[u32], op: u8, bytes: u8) -> NativeProgram {
    let mut w = vec![sop1(S_MOV_B32, 5, k(0) as u8)];
    // s_load_dwordx4 s[8:11], s[4:5], 0: the buffer's descriptor.
    w.extend_from_slice(&[0x3D << 26 | 0x02 << 18 | 8 << 6 | 4 >> 1, 125 << 25]);
    w.push(vop2(V_AND_B32, 1, k(7), 0));
    w.push(vop2(V_LSHLREV_B32, 2, k(bytes.trailing_zeros() as u8), 1));
    w.extend_from_slice(data);
    w.extend_from_slice(&mubuf(op, 3, 2, 8));
    w.push(S_ENDPGM);
    native(w, 4)
}

fn native(w: Vec<u32>, descriptor_dwords: u8) -> NativeProgram {
    NativeProgram {
        stage: HwStage::Cs,
        wave64: false,
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
            dwords: descriptor_dwords,
            user_data: 4,
            slot: 1,
        }],
        io: StageIo::Cs(ComputeIo {
            rsrc2: 5 << 1,
            threads: [64, 1, 1],
        }),
        code: w.iter().flat_map(|x| x.to_le_bytes()).collect(),
    }
}

/// Runs `native` on the hardware GPU and checks each texel (low, high).
fn check(native: &NativeProgram, want: impl Fn(u32, u32, u32) -> bool) {
    check_on(native, &View::Uint2TextureUav(8, 1), want);
}

/// [`check`] with the UAV `view` (8 slots of two dwords).
fn check_on(native: &NativeProgram, view: &View, want: impl Fn(u32, u32, u32) -> bool) {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; skipped");
        return;
    };
    if !host.int64_typed_atomics() {
        eprintln!("SKIPPED: {} has no 64-bit typed atomics", host.adapter());
        return;
    }
    let label = host.adapter();
    let shader = try_lower(native, false).unwrap_or_else(|e| panic!("lower: {e}"));
    let (root, pso) = host
        .compute_pipeline(&shader)
        .unwrap_or_else(|e| panic!("{label}: compute_pipeline failed: {e}"));
    let mut ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
    for u in &shader.user_data {
        ud[u.index as usize] = match (u.root, u.slot) {
            (RootType::Constants, 0) | (RootType::Table, 1) => 0,
            other => panic!("unexpected user data {other:?}"),
        };
    }
    let out = host
        .dispatch(
            &root,
            &pso,
            &Dispatch {
                roots: vec![(0, Root::Constants(ud))],
                views: std::slice::from_ref(view),
                samplers: &[],
                groups: [1, 1, 1],
            },
        )
        .unwrap_or_else(|e| panic!("{label}: dispatch failed: {e}"));
    for x in 0..8u32 {
        let (lo, hi) = (out[0][2 * x as usize], out[0][2 * x as usize + 1]);
        assert!(want(x, lo, hi), "{label}: texel {x} = ({lo:#x}, {hi:#x}); all: {:x?}", &out[0][..16]);
    }
}

#[test]
fn image_atomic_add_carries_into_the_high_dword() {
    // Eight threads per texel add 0xFFFFFFFF: 0x7_FFFFFFF8 only with 64 bits.
    let data = [vop1(V_MOV_B32, 3, MINUS_ONE), vop1(V_MOV_B32, 4, k(0))];
    check(&program(&data, IMAGE_ATOMIC_ADD, 0x3), |_, lo, hi| {
        (lo, hi) == (0xFFFF_FFF8, 7)
    });
}

#[test]
fn image_atomic_umax_compares_the_high_dword_first() {
    // Thread t: (low 100 + t, high t); the largest is thread x + 56's.
    let data = [vop2(V_ADD_NC_U32, 3, LITERAL, 0), 100, vop1(V_MOV_B32, 4, vgpr(0))];
    check(&program(&data, IMAGE_ATOMIC_UMAX, 0x3), |x, lo, hi| {
        (lo, hi) == (156 + x, 56 + x)
    });
}

#[test]
fn image_atomic_cmpswap_swaps_once_per_texel() {
    // new = (t + 1, 7), compare = 0: the first thread of each texel wins.
    let data = [
        vop2(V_ADD_NC_U32, 3, k(1), 0),
        vop1(V_MOV_B32, 4, k(7)),
        vop1(V_MOV_B32, 5, k(0)),
        vop1(V_MOV_B32, 6, k(0)),
    ];
    check(&program(&data, IMAGE_ATOMIC_CMPSWAP, 0xF), |x, lo, hi| {
        hi == 7 && (1..=64).contains(&lo) && (lo - 1) & 7 == x
    });
}

/// Data for the 64-bit cases, shared by images and buffers.
const ALL_ONES_LOW: [u32; 2] = [vop1(V_MOV_B32, 3, MINUS_ONE), vop1(V_MOV_B32, 4, k(0))];

#[test]
fn buffer_atomic_add_x2_carries_into_the_high_dword() {
    check_on(
        &buffer_program(&ALL_ONES_LOW, BUFFER_ATOMIC_ADD_X2, 8),
        &View::RawUav(16),
        |_, lo, hi| (lo, hi) == (0xFFFF_FFF8, 7),
    );
}

#[test]
fn buffer_atomic_sub_x2_borrows_from_the_high_dword() {
    // Eight threads per word subtract 1 from 0.
    let one = [vop1(V_MOV_B32, 3, k(1)), vop1(V_MOV_B32, 4, k(0))];
    check_on(
        &buffer_program(&one, BUFFER_ATOMIC_SUB_X2, 8),
        &View::RawUav(16),
        |_, lo, hi| (lo, hi) == (0xFFFF_FFF8, 0xFFFF_FFFF),
    );
}

#[test]
fn buffer_atomic_umax_x2_and_cmpswap_x2() {
    let data = [vop2(V_ADD_NC_U32, 3, LITERAL, 0), 100, vop1(V_MOV_B32, 4, vgpr(0))];
    check_on(
        &buffer_program(&data, BUFFER_ATOMIC_UMAX_X2, 8),
        &View::RawUav(16),
        |x, lo, hi| (lo, hi) == (156 + x, 56 + x),
    );
    let data = [
        vop2(V_ADD_NC_U32, 3, k(1), 0),
        vop1(V_MOV_B32, 4, k(7)),
        vop1(V_MOV_B32, 5, k(0)),
        vop1(V_MOV_B32, 6, k(0)),
    ];
    check_on(
        &buffer_program(&data, BUFFER_ATOMIC_CMPSWAP_X2, 8),
        &View::RawUav(16),
        |x, lo, hi| hi == 7 && (1..=64).contains(&lo) && (lo - 1) & 7 == x,
    );
}

#[test]
fn buffer_atomic_sub_of_a_dword() {
    // 32-bit: eight threads per dword subtract 1 from 0 (dwords 0..7, read
    // here as pairs 0..3); dwords 8..15 stay 0.
    let one = [vop1(V_MOV_B32, 3, k(1))];
    check_on(
        &buffer_program(&one, BUFFER_ATOMIC_SUB, 4),
        &View::RawUav(16),
        |x, lo, hi| if x < 4 { (lo, hi) == (0xFFFF_FFF8, 0xFFFF_FFF8) } else { (lo, hi) == (0, 0) },
    );
}
