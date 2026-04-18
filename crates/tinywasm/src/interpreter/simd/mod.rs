#![cfg_attr(feature = "simd-x86", allow(unsafe_code))]

#[macro_use]
mod macros;
mod instructions;
#[cfg(test)]
mod tests;
mod utils;

#[cfg(target_arch = "wasm32")]
use core::arch::wasm32 as wasm;
#[cfg(target_arch = "wasm64")]
use core::arch::wasm64 as wasm;

use crate::MemValue;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
/// A 128-bit SIMD value
pub struct Value128(pub(super) [u8; 16]);

impl From<[u8; 16]> for Value128 {
    fn from(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

impl MemValue<16> for Value128 {
    #[inline(always)]
    fn from_mem_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[inline(always)]
    fn to_mem_bytes(self) -> [u8; 16] {
        self.0
    }
}

impl From<Value128> for i128 {
    fn from(val: Value128) -> Self {
        i128::from_le_bytes(val.0)
    }
}

impl From<i128> for Value128 {
    fn from(value: i128) -> Self {
        Self(value.to_le_bytes())
    }
}

#[cfg_attr(any(target_arch = "wasm32", target_arch = "wasm64"), allow(unreachable_code))]
impl Value128 {
    #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
    #[inline(always)]
    fn to_wasm_v128(self) -> wasm::v128 {
        let b = self.0;
        wasm::u8x16(
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15],
        )
    }

    #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
    #[inline(always)]
    #[rustfmt::skip]
    fn from_wasm_v128(value: wasm::v128) -> Self {
        Self([ wasm::u8x16_extract_lane::<0>(value), wasm::u8x16_extract_lane::<1>(value), wasm::u8x16_extract_lane::<2>(value), wasm::u8x16_extract_lane::<3>(value), wasm::u8x16_extract_lane::<4>(value), wasm::u8x16_extract_lane::<5>(value), wasm::u8x16_extract_lane::<6>(value), wasm::u8x16_extract_lane::<7>(value), wasm::u8x16_extract_lane::<8>(value), wasm::u8x16_extract_lane::<9>(value), wasm::u8x16_extract_lane::<10>(value), wasm::u8x16_extract_lane::<11>(value), wasm::u8x16_extract_lane::<12>(value), wasm::u8x16_extract_lane::<13>(value), wasm::u8x16_extract_lane::<14>(value), wasm::u8x16_extract_lane::<15>(value)])
    }

    impl_lane_accessors! {
        as_i8x16 => from_i8x16: i8, 16, 1;
        as_u8x16 => from_u8x16: u8, 16, 1;
        as_i16x8 => from_i16x8: i16, 8, 2;
        as_u16x8 => from_u16x8: u16, 8, 2;
        as_i32x4 => pub from_i32x4: i32, 4, 4;
        as_u32x4 => from_u32x4: u32, 4, 4;
        as_f32x4 => from_f32x4: f32, 4, 4;
        as_i64x2 => pub from_i64x2: i64, 2, 8;
        as_u64x2 => from_u64x2: u64, 2, 8;
        as_f64x2 => from_f64x2: f64, 2, 8;
    }

    #[inline]
    fn map_f32x4(self, mut op: impl FnMut(f32) -> f32) -> Self {
        let bytes = self.0;
        let mut out_bytes = [0u8; 16];
        for (src, dst) in bytes.chunks_exact(4).zip(out_bytes.chunks_exact_mut(4)) {
            let lane = f32::from_bits(u32::from_le_bytes([src[0], src[1], src[2], src[3]]));
            dst.copy_from_slice(&op(lane).to_bits().to_le_bytes());
        }
        Self(out_bytes)
    }

    #[inline]
    fn zip_f32x4(self, rhs: Self, mut op: impl FnMut(f32, f32) -> f32) -> Self {
        let a_bytes = self.0;
        let b_bytes = rhs.0;
        let mut out_bytes = [0u8; 16];

        for ((a, b), dst) in a_bytes.chunks_exact(4).zip(b_bytes.chunks_exact(4)).zip(out_bytes.chunks_exact_mut(4)) {
            let a_lane = f32::from_bits(u32::from_le_bytes([a[0], a[1], a[2], a[3]]));
            let b_lane = f32::from_bits(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            dst.copy_from_slice(&op(a_lane, b_lane).to_bits().to_le_bytes());
        }

        Self(out_bytes)
    }

    #[inline]
    fn map_f64x2(self, mut op: impl FnMut(f64) -> f64) -> Self {
        let bytes = self.0;
        let mut out_bytes = [0u8; 16];
        for (src, dst) in bytes.chunks_exact(8).zip(out_bytes.chunks_exact_mut(8)) {
            let lane =
                f64::from_bits(u64::from_le_bytes([src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7]]));
            dst.copy_from_slice(&op(lane).to_bits().to_le_bytes());
        }
        Self(out_bytes)
    }

    #[inline]
    fn zip_f64x2(self, rhs: Self, mut op: impl FnMut(f64, f64) -> f64) -> Self {
        let a_bytes = self.0;
        let b_bytes = rhs.0;
        let mut out_bytes = [0u8; 16];

        for ((a, b), dst) in a_bytes.chunks_exact(8).zip(b_bytes.chunks_exact(8)).zip(out_bytes.chunks_exact_mut(8)) {
            let a_lane = f64::from_bits(u64::from_le_bytes([a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]]));
            let b_lane = f64::from_bits(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
            dst.copy_from_slice(&op(a_lane, b_lane).to_bits().to_le_bytes());
        }

        Self(out_bytes)
    }
}
