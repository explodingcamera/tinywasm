mod instructions;
mod macros;

use macros::{impl_lane_accessors, lane_read, lane_write};

#[cfg(target_arch = "wasm32")]
use core::arch::wasm32 as wasm;
#[cfg(target_arch = "wasm64")]
use core::arch::wasm64 as wasm;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
/// A 128-bit SIMD value
pub struct Value128(pub(crate) [u8; 16]);

/// Vector representation used by tail dispatch and x86 intrinsics.
#[cfg(any(tw_tail_calls, tw_simd_backend = "x86"))]
pub(crate) type NativeVector = core::cfg_select! {
    tw_simd_backend = "x86" => core::arch::x86_64::__m128i,
    all(target_arch = "aarch64", target_feature = "neon") => core::arch::aarch64::uint8x16_t,
    all(target_arch = "wasm32", target_feature = "simd128") => core::arch::wasm32::v128,
    all(target_arch = "wasm64", target_feature = "simd128") => core::arch::wasm64::v128,
    _ => Value128,
};

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

#[cfg(any(tw_tail_calls, tw_simd_backend = "x86"))]
impl Value128 {
    /// Converts raw vector bits to the target's dispatch representation.
    #[inline(always)]
    pub(crate) fn to_native(self) -> NativeVector {
        let bytes: [u8; 16] = self.0;
        core::cfg_select! {
            all(any(target_arch = "wasm32", target_arch = "wasm64"), target_feature = "simd128") => {
                Self(bytes).to_wasm_v128()
            }
            any(tw_simd_backend = "x86", all(target_arch = "aarch64", target_feature = "neon")) => {
                // SAFETY: the carrier is exactly 16 bytes and permits all bit patterns.
                #[expect(unsafe_code)]
                unsafe {
                    core::mem::transmute::<[u8; 16], NativeVector>(bytes)
                }
            }
            _ => Self(bytes),
        }
    }

    /// Converts the target's dispatch representation back to raw vector bits.
    #[inline(always)]
    pub(crate) fn from_native(value: NativeVector) -> Self {
        core::cfg_select! {
            all(any(target_arch = "wasm32", target_arch = "wasm64"), target_feature = "simd128") => {
                Self::from_wasm_v128(value)
            }
            any(tw_simd_backend = "x86", all(target_arch = "aarch64", target_feature = "neon")) => {
                // SAFETY: the carrier contains exactly 16 initialized bytes.
                #[expect(unsafe_code)]
                let bytes: [u8; 16] = unsafe { core::mem::transmute::<NativeVector, [u8; 16]>(value) };
                Self(bytes)
            }
            _ => value,
        }
    }
}

impl Value128 {
    fn extract_lane_bytes<const BYTES: usize, const LANE: usize>(self, lane: u8) -> [u8; BYTES] {
        const { assert!(BYTES * LANE == 16) };
        let start = lane as usize * BYTES;
        core::array::from_fn(|i| self.0[start + i])
    }

    fn replace_lane_bytes<const BYTES: usize, const LANE: usize>(self, lane: u8, value: [u8; BYTES]) -> Self {
        const { assert!(BYTES * LANE == 16) };
        let mut bytes = self.0;
        let start = lane as usize * BYTES;
        bytes[start..start + BYTES].copy_from_slice(&value);
        Self(bytes)
    }

    impl_lane_accessors! {
        as_i8x16 => from_i8x16: i8, 16, 1;
        as_u8x16 => from_u8x16: u8, 16, 1;
        as_i16x8 => from_i16x8: i16, 8, 2;
        as_u16x8 => from_u16x8: u16, 8, 2;
        as_i32x4 => from_i32x4: i32, 4, 4;
        as_u32x4 => from_u32x4: u32, 4, 4;
        as_f32x4 => from_f32x4: f32, 4, 4;
        as_i64x2 => from_i64x2: i64, 2, 8;
        as_u64x2 => from_u64x2: u64, 2, 8;
        as_f64x2 => from_f64x2: f64, 2, 8;
    }

    #[cfg(tw_simd_backend = "wasm")]
    #[inline(always)]
    fn to_wasm_v128(self) -> wasm::v128 {
        let b = self.0;
        wasm::u8x16(
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15],
        )
    }

    #[cfg(tw_simd_backend = "wasm")]
    #[inline(always)]
    #[rustfmt::skip]
    fn from_wasm_v128(value: wasm::v128) -> Self {
        Self([ wasm::u8x16_extract_lane::<0>(value), wasm::u8x16_extract_lane::<1>(value), wasm::u8x16_extract_lane::<2>(value), wasm::u8x16_extract_lane::<3>(value), wasm::u8x16_extract_lane::<4>(value), wasm::u8x16_extract_lane::<5>(value), wasm::u8x16_extract_lane::<6>(value), wasm::u8x16_extract_lane::<7>(value), wasm::u8x16_extract_lane::<8>(value), wasm::u8x16_extract_lane::<9>(value), wasm::u8x16_extract_lane::<10>(value), wasm::u8x16_extract_lane::<11>(value), wasm::u8x16_extract_lane::<12>(value), wasm::u8x16_extract_lane::<13>(value), wasm::u8x16_extract_lane::<14>(value), wasm::u8x16_extract_lane::<15>(value)])
    }

    #[inline]
    fn map_f32x4(self, mut op: impl FnMut(f32) -> f32) -> Self {
        let [a, b, c, d] = self.as_f32x4();
        Self::from_f32x4([op(a), op(b), op(c), op(d)])
    }

    #[inline]
    fn zip_f32x4(self, rhs: Self, mut op: impl FnMut(f32, f32) -> f32) -> Self {
        let [a0, a1, a2, a3] = self.as_f32x4();
        let [b0, b1, b2, b3] = rhs.as_f32x4();
        Self::from_f32x4([op(a0, b0), op(a1, b1), op(a2, b2), op(a3, b3)])
    }

    #[inline]
    fn map_f64x2(self, mut op: impl FnMut(f64) -> f64) -> Self {
        let [a, b] = self.as_f64x2();
        Self::from_f64x2([op(a), op(b)])
    }

    #[inline]
    fn zip_f64x2(self, rhs: Self, mut op: impl FnMut(f64, f64) -> f64) -> Self {
        let [a0, a1] = self.as_f64x2();
        let [b0, b1] = rhs.as_f64x2();
        Self::from_f64x2([op(a0, b0), op(a1, b1)])
    }
}
