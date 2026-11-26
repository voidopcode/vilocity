//! Typed buffer accesses through views of 16-bit integer formats on the
//! host GPU. The title's face network (`compute_graph_ops`) keeps its
//! halves in R16_UINT and R16G16B16A16_UINT views and moves them with
//! `buffer_load_format_d16_*` and `buffer_store_format_d16_*`: through an
//! integer view the console moves the 16-bit integers as they are, not as
//! halves. The lowering reads and writes such views through float-typed
//! handles; these tests also hold the host to passing their bits.

use super::host::{Dispatch, Host, Root, Stride, View};
use super::split_tests::{S_ENDPGM, k, sop1, try_lower, vop1, vop2, V_MOV_B32};
use super::valu_tests::ONE_AT_A_TIME;
use super::{VIEW_INTEGER, VIEW_SIGNED};
use crate::native::{ComputeIo, HwStage, NativeProgram, ResourceType, RootParam, RootType, StageIo, UsedResource};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R16G16B16A16_SINT, DXGI_FORMAT_R16G16B16A16_UINT,
};

const S_MOV_B32: u8 = 0x03;
const V_LSHLREV_B32: u8 = 0x1A;
const LITERAL: u16 = 255;
const BUFFER_LOAD_FORMAT_X: u16 = 0x00;
const BUFFER_LOAD_FORMAT_D16_X: u16 = 0x80;
const BUFFER_STORE_FORMAT_D16_XYZW: u16 = 0x87;

/// MUBUF `op v[vdata], v[vaddr], s[srsrc:srsrc+3], 0 idxen`.
const fn mubuf_idxen(op: u16, vdata: u8, vaddr: u8, srsrc: u8) -> [u32; 2] {
    [
        0x38 << 26 | ((op >> 7) as u32 & 1) << 25 | (op as u32 & 0x7F) << 18 | 1 << 13,
        0x80 << 24 | (srsrc as u32 / 4) << 16 | (vdata as u32) << 8 | vaddr as u32,
    ]
}

/// A wave64 program of one 64-thread group: the raw UAV at heap index 0
/// (s[8:11]), a typed `kind` view at heap index 1 (s[12:15]); `body`
/// leaves each thread's result in v1, stored at dword `v0` of the UAV.
fn program(body: &[u32], kind: ResourceType) -> NativeProgram {
    let mut w = vec![sop1(S_MOV_B32, 5, k(0) as u8)];
    // s_load_dwordx4 s[8:11], s[4:5], 0 and s[12:15], s[4:5], 32.
    w.extend_from_slice(&[0x3D << 26 | 0x02 << 18 | 8 << 6 | 4 >> 1, 125 << 25]);
    w.extend_from_slice(&[0x3D << 26 | 0x02 << 18 | 12 << 6 | 4 >> 1, 125 << 25 | 32]);
    w.extend_from_slice(body);
    w.push(vop2(V_LSHLREV_B32, 2, k(2), 0));
    // buffer_store_dword v1, v2, s[8:11], 0 offen
    w.extend_from_slice(&[0x38 << 26 | 0x1C << 18 | 1 << 12, 0x80 << 24 | 2 << 16 | 1 << 8 | 2]);
    w.push(S_ENDPGM);
    let table = |kind, dwords| UsedResource {
        kind,
        root: RootType::Table,
        table_offset_dwords: dwords,
        lower_bound: 0,
        stride: 0,
        dwords: 4,
        user_data: 4,
        slot: 1,
    };
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
        used: vec![table(ResourceType::Uav, 0), table(kind, 8)],
        io: StageIo::Cs(ComputeIo {
            rsrc2: 5 << 1,
            threads: [64, 1, 1],
        }),
        code: w.iter().flat_map(|x| x.to_le_bytes()).collect(),
    }
}

/// Runs `native` with `typed` at heap index 1, its stride-table dword
/// `word`; the UAVs' dwords.
fn run(native: &NativeProgram, typed: View, word: u32) -> Option<Vec<Vec<u32>>> {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(host) = Host::new(false) else {
        eprintln!("no SM 6.6 hardware device; skipped");
        return None;
    };
    // The split form: the one NVIDIA runs, and AMD's too.
    let shader = try_lower(native, true).expect("lower");
    let (root, pso) = host.compute_pipeline(&shader).expect("compute_pipeline");
    let ud = vec![0u32; shader.user_data_dwords().max(1) as usize];
    let mut roots = vec![(0, Root::Constants(ud))];
    if !shader.root_srvs.is_empty() {
        roots.push((1, Root::View { srv: true, data: vec![4, word] }));
    }
    let out = host
        .dispatch(
            &root,
            &pso,
            &Dispatch {
                roots,
                views: &[View::RawUav(64), typed],
                samplers: &[],
                groups: [1, 1, 1],
            },
        )
        .expect("dispatch");
    Some(out)
}

/// Element `t` of the R16_UINT view: bit patterns no f16 round trip keeps.
fn element(t: u32) -> u32 {
    0x8001 + 0x0123 * t
}

/// Thread t loads element t of an R16_UINT view with `op`.
fn load(op: u16) -> Option<Vec<u32>> {
    let data: Vec<u32> = (0..32).map(|i| element(2 * i) | element(2 * i + 1) << 16).collect();
    let typed = View::Buffer {
        data: &data,
        stride: Stride::Format(DXGI_FORMAT_R16_UINT, 2),
    };
    let native = program(&mubuf_idxen(op, 1, 0, 12), ResourceType::Srv);
    run(&native, typed, 2 | VIEW_INTEGER).map(|out| out[0][..64].to_vec())
}

#[test]
fn integer_view_format_load_passes_bits() {
    // What the d16 forms rely on: a float-typed load of an integer view
    // gives the integer's bits.
    if let Some(got) = load(BUFFER_LOAD_FORMAT_X) {
        assert_eq!(got, (0..64).map(element).collect::<Vec<_>>());
    }
}

#[test]
fn integer_view_d16_load_keeps_bits() {
    if let Some(got) = load(BUFFER_LOAD_FORMAT_D16_X) {
        assert_eq!(got, (0..64).map(element).collect::<Vec<_>>());
    }
}

#[test]
fn integer_view_d16_store_keeps_bits() {
    // v2, v3: four 16-bit channels, then element t of the typed UAV.
    let (lo, hi) = (0x8001_FFFF, 0x1234_C3A5);
    let mut body = vec![vop1(V_MOV_B32, 2, LITERAL), lo, vop1(V_MOV_B32, 3, LITERAL), hi];
    body.extend(mubuf_idxen(BUFFER_STORE_FORMAT_D16_XYZW, 2, 0, 12));
    body.push(vop1(V_MOV_B32, 1, k(0)));
    let native = program(&body, ResourceType::Uav);
    let formats: [(DXGI_FORMAT, u32); 2] = [
        (DXGI_FORMAT_R16G16B16A16_UINT, VIEW_INTEGER),
        (DXGI_FORMAT_R16G16B16A16_SINT, VIEW_INTEGER | VIEW_SIGNED),
    ];
    for (format, kind) in formats {
        let Some(out) = run(&native, View::TypedUav(format, 64, 8), 8 | kind) else {
            return;
        };
        let want: Vec<u32> = (0..64).flat_map(|_| [lo, hi]).collect();
        assert_eq!(out[1], want, "{format:?}");
    }
}
