pub(super) use core::ops::Neg;

pub(super) use core::simd::Simd;
pub(super) use core::simd::ToBytes;
pub(super) use core::simd::num::SimdFloat;
pub(super) use core::simd::num::SimdInt;

macro_rules! impl_wasm_simd_val {
    ($($v:ident),*) => {
        $(
            pub(super) fn $v(f: core::simd::u8x16) -> core::simd::$v {
                core::simd::$v::from_ne_bytes(f)
            }
        )*
    };
}

impl_wasm_simd_val!(i8x16, i16x8, i32x4, i64x2, f32x4, f64x2);
