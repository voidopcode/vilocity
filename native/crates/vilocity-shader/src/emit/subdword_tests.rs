//! Byte and halfword access to LDS and buffers on the host GPU: 64 threads
//! write neighbouring bytes (or halves) of the same dwords at once, then
//! each reads another thread's back, zero- or sign-extended. (The console
//! BVH builder's shaders use these.)

use super::split_tests::{
    V_MAD_U32_U24, V_SUB_NC_U32, check_in, k, program_in, sopp, vgpr, vop2, vop3_3,
};

const S_BARRIER: u8 = 0x0A;
const V_LSHLREV_B32: u8 = 0x1A;
const DS_WRITE_B8: u8 = 0x1E;
const DS_WRITE_B16: u8 = 0x1F;
const DS_READ_I8: u8 = 0x39;
const DS_READ_U8: u8 = 0x3A;
const DS_READ_I16: u8 = 0x3B;
const DS_READ_U16: u8 = 0x3C;
const BUFFER_LOAD_UBYTE: u8 = 0x08;
const BUFFER_LOAD_SBYTE: u8 = 0x09;
const BUFFER_LOAD_USHORT: u8 = 0x0A;
const BUFFER_LOAD_SSHORT: u8 = 0x0B;
const BUFFER_STORE_BYTE: u8 = 0x18;
const BUFFER_STORE_SHORT: u8 = 0x1A;
const LITERAL: u16 = 255;

/// DS `op vdst, addr, data0` (offset 0).
const fn ds(op: u8, vdst: u8, addr: u8, data0: u8) -> [u32; 2] {
    [
        0x36 << 26 | (op as u32) << 18,
        (vdst as u32) << 24 | (data0 as u32) << 8 | addr as u32,
    ]
}

/// MUBUF `op v[vdata], v[vaddr], s[8:11], 0 offen` (the UAV at heap 0).
const fn mubuf(op: u8, vdata: u8, vaddr: u8) -> [u32; 2] {
    [
        0x38 << 26 | (op as u32) << 18 | 1 << 12,
        0x80 << 24 | 2 << 16 | (vdata as u32) << 8 | vaddr as u32,
    ]
}

/// Thread t's value: `t * mul + 1`, masked to `bits`.
fn value(t: u32, mul: u32, bits: u32) -> u32 {
    (t * mul + 1) & ((1 << bits) - 1)
}

/// The value thread 63 − t wrote, as a load of `bits` extends it.
fn read_back(t: u32, mul: u32, bits: u32, signed: bool) -> u32 {
    let v = value(63 - t, mul, bits);
    if signed {
        ((v << (32 - bits)) as i32 >> (32 - bits)) as u32
    } else {
        v
    }
}

/// v3 = t * mul + 1; v2 = the thread's element's byte address (t · bytes);
/// v4 = thread 63 − t's.
fn setup(mul: u32, bytes: u8) -> Vec<u32> {
    let shift = k(bytes.trailing_zeros() as u8);
    let [a, b] = vop3_3(V_MAD_U32_U24, 3, vgpr(0), LITERAL, k(1));
    vec![
        a,
        b,
        mul,
        vop2(V_LSHLREV_B32, 2, shift, 0),
        vop2(V_SUB_NC_U32, 4, k(63), 0),
        vop2(V_LSHLREV_B32, 4, shift, 4),
    ]
}

/// Writes through LDS and reads back into v1.
fn lds(write: u8, read: u8, mul: u32, bytes: u8) -> Vec<u32> {
    let mut w = setup(mul, bytes);
    w.extend(ds(write, 0, 2, 3));
    w.push(sopp(S_BARRIER, 0));
    w.extend(ds(read, 1, 4, 0));
    w
}

#[test]
fn lds_bytes_and_halves() {
    for (read, signed) in [(DS_READ_U8, false), (DS_READ_I8, true)] {
        check_in(&program_in(&lds(DS_WRITE_B8, read, 3, 1), [64, 1, 1], 1), |t| {
            read_back(t, 3, 8, signed)
        });
    }
    for (read, signed) in [(DS_READ_U16, false), (DS_READ_I16, true)] {
        check_in(&program_in(&lds(DS_WRITE_B16, read, 1000, 2), [64, 1, 1], 1), |t| {
            read_back(t, 1000, 16, signed)
        });
    }
}

/// Writes through the output buffer's first bytes and reads back into v1;
/// a second barrier keeps the result stores (dword t) after every read.
fn buffer(write: u8, read: u8, mul: u32, bytes: u8) -> Vec<u32> {
    let mut w = setup(mul, bytes);
    w.extend(mubuf(write, 3, 2));
    w.push(sopp(S_BARRIER, 0));
    w.extend(mubuf(read, 1, 4));
    w.push(sopp(S_BARRIER, 0));
    w
}

#[test]
fn buffer_bytes_and_halves() {
    for (read, signed) in [(BUFFER_LOAD_UBYTE, false), (BUFFER_LOAD_SBYTE, true)] {
        check_in(&program_in(&buffer(BUFFER_STORE_BYTE, read, 3, 1), [64, 1, 1], 0), |t| {
            read_back(t, 3, 8, signed)
        });
    }
    for (read, signed) in [(BUFFER_LOAD_USHORT, false), (BUFFER_LOAD_SSHORT, true)] {
        check_in(&program_in(&buffer(BUFFER_STORE_SHORT, read, 1000, 2), [64, 1, 1], 0), |t| {
            read_back(t, 1000, 16, signed)
        });
    }
}
