use super::num_helpers::TinywasmFloatExt;

#[cfg(not(feature = "std"))]
use super::no_std_floats::NoStdFloatExt;
#[cfg(target_arch = "wasm32")]
use core::arch::wasm32 as wasm;
#[cfg(target_arch = "wasm64")]
use core::arch::wasm64 as wasm;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
/// A 128-bit SIMD value
pub struct Value128(i128);

impl From<Value128> for i128 {
    fn from(val: Value128) -> Self {
        val.0
    }
}

impl From<i128> for Value128 {
    fn from(value: i128) -> Self {
        Self(value)
    }
}

macro_rules! simd_wrapping_binop {
    ($name:ident, $doc:literal, $wasm_op:ident, $lane_ty:ty, $lane_count:expr, $as_lanes:ident, $from_lanes:ident, $op:ident) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            return Self::from_wasm_v128(wasm::$wasm_op(self.to_wasm_v128(), rhs.to_wasm_v128()));

            let a = self.$as_lanes();
            let b = rhs.$as_lanes();
            let mut out = [0 as $lane_ty; $lane_count];
            for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
                *dst = lhs.$op(rhs);
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_sat_binop {
    ($name:ident, $doc:literal, $wasm_op:ident, $lane_ty:ty, $lane_count:expr, $as_lanes:ident, $from_lanes:ident, $op:ident) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            return Self::from_wasm_v128(wasm::$wasm_op(self.to_wasm_v128(), rhs.to_wasm_v128()));

            let a = self.$as_lanes();
            let b = rhs.$as_lanes();
            let mut out = [0 as $lane_ty; $lane_count];
            for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
                *dst = lhs.$op(rhs);
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_shift_left {
    ($name:ident, $doc:literal, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $mask:expr) => {
        #[doc(alias = $doc)]
        pub fn $name(self, shift: u32) -> Self {
            let lanes = self.$as_lanes();
            let s = shift & $mask;
            let mut out = [0 as $lane_ty; $count];
            for (dst, lane) in out.iter_mut().zip(lanes) {
                *dst = lane.wrapping_shl(s);
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_shift_right {
    ($name:ident, $doc:literal, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $mask:expr) => {
        #[doc(alias = $doc)]
        pub fn $name(self, shift: u32) -> Self {
            let lanes = self.$as_lanes();
            let s = shift & $mask;
            let mut out = [0 as $lane_ty; $count];
            for (dst, lane) in out.iter_mut().zip(lanes) {
                *dst = lane >> s;
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_avgr_u {
    ($name:ident, $doc:literal, $wasm_op:ident, $lane_ty:ty, $wide_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            return Self::from_wasm_v128(wasm::$wasm_op(self.to_wasm_v128(), rhs.to_wasm_v128()));

            let a = self.$as_lanes();
            let b = rhs.$as_lanes();
            let mut out = [0 as $lane_ty; $count];
            for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
                *dst = ((lhs as $wide_ty + rhs as $wide_ty + 1) >> 1) as $lane_ty;
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_extend_cast {
    ($name:ident, $doc:literal, $src_as:ident, $dst_from:ident, $dst_ty:ty, $dst_count:expr, $offset:expr) => {
        #[doc(alias = $doc)]
        pub fn $name(self) -> Self {
            let lanes = self.$src_as();
            let mut out = [0 as $dst_ty; $dst_count];
            for (dst, src) in out.iter_mut().zip(lanes[$offset..($offset + $dst_count)].iter()) {
                *dst = *src as $dst_ty;
            }
            Self::$dst_from(out)
        }
    };
}

macro_rules! simd_extmul_signed {
    ($name:ident, $doc:literal, $src_as:ident, $dst_from:ident, $dst_ty:ty, $dst_count:expr, $offset:expr) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            let a = self.$src_as();
            let b = rhs.$src_as();
            let mut out = [0 as $dst_ty; $dst_count];
            for ((dst, lhs), rhs) in out
                .iter_mut()
                .zip(a[$offset..($offset + $dst_count)].iter())
                .zip(b[$offset..($offset + $dst_count)].iter())
            {
                *dst = (*lhs as $dst_ty).wrapping_mul(*rhs as $dst_ty);
            }
            Self::$dst_from(out)
        }
    };
}

macro_rules! simd_extmul_unsigned {
    ($name:ident, $doc:literal, $src_as:ident, $dst_from:ident, $dst_ty:ty, $dst_count:expr, $offset:expr) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            let a = self.$src_as();
            let b = rhs.$src_as();
            let mut out = [0 as $dst_ty; $dst_count];
            for ((dst, lhs), rhs) in out
                .iter_mut()
                .zip(a[$offset..($offset + $dst_count)].iter())
                .zip(b[$offset..($offset + $dst_count)].iter())
            {
                *dst = (*lhs as $dst_ty) * (*rhs as $dst_ty);
            }
            Self::$dst_from(out)
        }
    };
}

macro_rules! simd_cmp_mask {
    ($name:ident, $doc:literal, $wasm_op:ident, $out_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            return Self::from_wasm_v128(wasm::$wasm_op(self.to_wasm_v128(), rhs.to_wasm_v128()));

            let a = self.$as_lanes();
            let b = rhs.$as_lanes();
            let mut out = [0 as $out_ty; $count];
            for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
                *dst = if lhs $cmp rhs { -1 } else { 0 };
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_cmp_mask_const {
    ($name:ident, $doc:literal, $out_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            let a = self.$as_lanes();
            let b = rhs.$as_lanes();
            let mut out = [0 as $out_ty; $count];
            for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
                *dst = if lhs $cmp rhs { -1 } else { 0 };
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_cmp_delegate {
    ($name:ident, $doc:literal, $delegate:ident) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            rhs.$delegate(self)
        }
    };
}

macro_rules! simd_abs_const {
    ($name:ident, $doc:literal, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {
        #[doc(alias = $doc)]
        pub fn $name(self) -> Self {
            let a = self.$as_lanes();
            let mut out = [0 as $lane_ty; $count];
            for (dst, lane) in out.iter_mut().zip(a) {
                *dst = lane.wrapping_abs();
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_neg {
    ($name:ident, $doc:literal, $wasm_op:ident, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {
        #[doc(alias = $doc)]
        pub fn $name(self) -> Self {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            return Self::from_wasm_v128(wasm::$wasm_op(self.to_wasm_v128()));

            let a = self.$as_lanes();
            let mut out = [0 as $lane_ty; $count];
            for (dst, lane) in out.iter_mut().zip(a) {
                *dst = lane.wrapping_neg();
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_minmax {
    ($name:ident, $doc:literal, $wasm_op:ident, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
            return Self::from_wasm_v128(wasm::$wasm_op(self.to_wasm_v128(), rhs.to_wasm_v128()));

            let a = self.$as_lanes();
            let b = rhs.$as_lanes();
            let mut out = [0 as $lane_ty; $count];
            for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
                *dst = if lhs $cmp rhs { lhs } else { rhs };
            }
            Self::$from_lanes(out)
        }
    };
}

macro_rules! simd_float_unary {
    ($name:ident, $doc:literal, $map:ident, $op:expr) => {
        #[doc(alias = $doc)]
        pub fn $name(self) -> Self {
            self.$map($op)
        }
    };
}

macro_rules! simd_float_binary {
    ($name:ident, $doc:literal, $zip:ident, $op:expr) => {
        #[doc(alias = $doc)]
        pub fn $name(self, rhs: Self) -> Self {
            self.$zip(rhs, $op)
        }
    };
}

#[cfg_attr(any(target_arch = "wasm32", target_arch = "wasm64"), allow(unreachable_code))]
impl Value128 {
    #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
    #[inline(always)]
    fn to_wasm_v128(self) -> wasm::v128 {
        let b = self.to_le_bytes();
        wasm::u8x16(
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15],
        )
    }

    #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
    #[inline(always)]
    #[rustfmt::skip]
    fn from_wasm_v128(value: wasm::v128) -> Self {
        Self::from_le_bytes([ wasm::u8x16_extract_lane::<0>(value), wasm::u8x16_extract_lane::<1>(value), wasm::u8x16_extract_lane::<2>(value), wasm::u8x16_extract_lane::<3>(value), wasm::u8x16_extract_lane::<4>(value), wasm::u8x16_extract_lane::<5>(value), wasm::u8x16_extract_lane::<6>(value), wasm::u8x16_extract_lane::<7>(value), wasm::u8x16_extract_lane::<8>(value), wasm::u8x16_extract_lane::<9>(value), wasm::u8x16_extract_lane::<10>(value), wasm::u8x16_extract_lane::<11>(value), wasm::u8x16_extract_lane::<12>(value), wasm::u8x16_extract_lane::<13>(value), wasm::u8x16_extract_lane::<14>(value), wasm::u8x16_extract_lane::<15>(value)])
    }

    #[inline]
    pub const fn from_le_bytes(bytes: [u8; 16]) -> Self {
        Self(i128::from_le_bytes(bytes))
    }

    #[inline]
    pub const fn to_le_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_i8x16(self) -> [i8; 16] {
        let b = self.to_le_bytes();
        [b[0] as i8, b[1] as i8, b[2] as i8, b[3] as i8, b[4] as i8, b[5] as i8, b[6] as i8, b[7] as i8, b[8] as i8, b[9] as i8, b[10] as i8, b[11] as i8, b[12] as i8, b[13] as i8, b[14] as i8, b[15] as i8]
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_u8x16(self) -> [u8; 16] {
        self.to_le_bytes()
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_i8x16(x: [i8; 16]) -> Self {
        Self::from_le_bytes([x[0] as u8, x[1] as u8, x[2] as u8, x[3] as u8, x[4] as u8, x[5] as u8, x[6] as u8, x[7] as u8, x[8] as u8, x[9] as u8, x[10] as u8, x[11] as u8, x[12] as u8, x[13] as u8, x[14] as u8, x[15] as u8])
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_u8x16(x: [u8; 16]) -> Self {
        Self::from_le_bytes(x)
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_i16x8(self) -> [i16; 8] {
        let b = self.to_le_bytes();
        [i16::from_le_bytes([b[0], b[1]]), i16::from_le_bytes([b[2], b[3]]), i16::from_le_bytes([b[4], b[5]]), i16::from_le_bytes([b[6], b[7]]), i16::from_le_bytes([b[8], b[9]]), i16::from_le_bytes([b[10], b[11]]), i16::from_le_bytes([b[12], b[13]]), i16::from_le_bytes([b[14], b[15]])]
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_u16x8(self) -> [u16; 8] {
        let b = self.to_le_bytes();
        [u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]), u16::from_le_bytes([b[4], b[5]]), u16::from_le_bytes([b[6], b[7]]), u16::from_le_bytes([b[8], b[9]]), u16::from_le_bytes([b[10], b[11]]), u16::from_le_bytes([b[12], b[13]]), u16::from_le_bytes([b[14], b[15]])]
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_i16x8(x: [i16; 8]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[4].to_le_bytes()[0], x[4].to_le_bytes()[1], x[5].to_le_bytes()[0], x[5].to_le_bytes()[1], x[6].to_le_bytes()[0], x[6].to_le_bytes()[1], x[7].to_le_bytes()[0], x[7].to_le_bytes()[1]])
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_u16x8(x: [u16; 8]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[4].to_le_bytes()[0], x[4].to_le_bytes()[1], x[5].to_le_bytes()[0], x[5].to_le_bytes()[1], x[6].to_le_bytes()[0], x[6].to_le_bytes()[1], x[7].to_le_bytes()[0], x[7].to_le_bytes()[1]])
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_i32x4(self) -> [i32; 4] {
        let b = self.to_le_bytes();
        [i32::from_le_bytes([b[0], b[1], b[2], b[3]]), i32::from_le_bytes([b[4], b[5], b[6], b[7]]), i32::from_le_bytes([b[8], b[9], b[10], b[11]]), i32::from_le_bytes([b[12], b[13], b[14], b[15]])]
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_u32x4(self) -> [u32; 4] {
        let b = self.to_le_bytes();
        [u32::from_le_bytes([b[0], b[1], b[2], b[3]]), u32::from_le_bytes([b[4], b[5], b[6], b[7]]), u32::from_le_bytes([b[8], b[9], b[10], b[11]]), u32::from_le_bytes([b[12], b[13], b[14], b[15]])]
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_f32x4(self) -> [f32; 4] {
        let b = self.to_le_bytes();
        [f32::from_bits(u32::from_le_bytes([b[0], b[1], b[2], b[3]])), f32::from_bits(u32::from_le_bytes([b[4], b[5], b[6], b[7]])), f32::from_bits(u32::from_le_bytes([b[8], b[9], b[10], b[11]])), f32::from_bits(u32::from_le_bytes([b[12], b[13], b[14], b[15]]))]
    }

    #[inline]
    #[rustfmt::skip]
    pub const fn from_i32x4(x: [i32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[2].to_le_bytes()[2], x[2].to_le_bytes()[3], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[3].to_le_bytes()[2], x[3].to_le_bytes()[3]])
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_u32x4(x: [u32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[2].to_le_bytes()[2], x[2].to_le_bytes()[3], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[3].to_le_bytes()[2], x[3].to_le_bytes()[3]])
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_f32x4(x: [f32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_bits().to_le_bytes()[0], x[0].to_bits().to_le_bytes()[1], x[0].to_bits().to_le_bytes()[2], x[0].to_bits().to_le_bytes()[3], x[1].to_bits().to_le_bytes()[0], x[1].to_bits().to_le_bytes()[1], x[1].to_bits().to_le_bytes()[2], x[1].to_bits().to_le_bytes()[3], x[2].to_bits().to_le_bytes()[0], x[2].to_bits().to_le_bytes()[1], x[2].to_bits().to_le_bytes()[2], x[2].to_bits().to_le_bytes()[3], x[3].to_bits().to_le_bytes()[0], x[3].to_bits().to_le_bytes()[1], x[3].to_bits().to_le_bytes()[2], x[3].to_bits().to_le_bytes()[3]])
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_i64x2(self) -> [i64; 2] {
        let b = self.to_le_bytes();
        [i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), i64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]])]
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_u64x2(self) -> [u64; 2] {
        let b = self.to_le_bytes();
        [u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]])]
    }

    #[inline]
    #[rustfmt::skip]
    const fn as_f64x2(self) -> [f64; 2] {
        let b = self.to_le_bytes();
        [f64::from_bits(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])), f64::from_bits(u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]))]
    }

    #[inline]
    #[rustfmt::skip]
    pub const fn from_i64x2(x: [i64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[0].to_le_bytes()[4], x[0].to_le_bytes()[5], x[0].to_le_bytes()[6], x[0].to_le_bytes()[7], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[1].to_le_bytes()[4], x[1].to_le_bytes()[5], x[1].to_le_bytes()[6], x[1].to_le_bytes()[7]])
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_u64x2(x: [u64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[0].to_le_bytes()[4], x[0].to_le_bytes()[5], x[0].to_le_bytes()[6], x[0].to_le_bytes()[7], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[1].to_le_bytes()[4], x[1].to_le_bytes()[5], x[1].to_le_bytes()[6], x[1].to_le_bytes()[7]])
    }

    #[inline]
    #[rustfmt::skip]
    const fn from_f64x2(x: [f64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_bits().to_le_bytes()[0], x[0].to_bits().to_le_bytes()[1], x[0].to_bits().to_le_bytes()[2], x[0].to_bits().to_le_bytes()[3], x[0].to_bits().to_le_bytes()[4], x[0].to_bits().to_le_bytes()[5], x[0].to_bits().to_le_bytes()[6], x[0].to_bits().to_le_bytes()[7], x[1].to_bits().to_le_bytes()[0], x[1].to_bits().to_le_bytes()[1], x[1].to_bits().to_le_bytes()[2], x[1].to_bits().to_le_bytes()[3], x[1].to_bits().to_le_bytes()[4], x[1].to_bits().to_le_bytes()[5], x[1].to_bits().to_le_bytes()[6], x[1].to_bits().to_le_bytes()[7]])
    }

    #[inline]
    fn map_f32x4(self, mut op: impl FnMut(f32) -> f32) -> Self {
        let bytes = self.to_le_bytes();
        let mut out_bytes = [0u8; 16];
        for (src, dst) in bytes.chunks_exact(4).zip(out_bytes.chunks_exact_mut(4)) {
            let lane = f32::from_bits(u32::from_le_bytes([src[0], src[1], src[2], src[3]]));
            dst.copy_from_slice(&op(lane).to_bits().to_le_bytes());
        }
        Self::from_le_bytes(out_bytes)
    }

    #[inline]
    fn zip_f32x4(self, rhs: Self, mut op: impl FnMut(f32, f32) -> f32) -> Self {
        let a_bytes = self.to_le_bytes();
        let b_bytes = rhs.to_le_bytes();
        let mut out_bytes = [0u8; 16];

        for ((a, b), dst) in a_bytes.chunks_exact(4).zip(b_bytes.chunks_exact(4)).zip(out_bytes.chunks_exact_mut(4)) {
            let a_lane = f32::from_bits(u32::from_le_bytes([a[0], a[1], a[2], a[3]]));
            let b_lane = f32::from_bits(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            dst.copy_from_slice(&op(a_lane, b_lane).to_bits().to_le_bytes());
        }

        Self::from_le_bytes(out_bytes)
    }

    #[inline]
    fn map_f64x2(self, mut op: impl FnMut(f64) -> f64) -> Self {
        let bytes = self.to_le_bytes();
        let mut out_bytes = [0u8; 16];
        for (src, dst) in bytes.chunks_exact(8).zip(out_bytes.chunks_exact_mut(8)) {
            let lane =
                f64::from_bits(u64::from_le_bytes([src[0], src[1], src[2], src[3], src[4], src[5], src[6], src[7]]));
            dst.copy_from_slice(&op(lane).to_bits().to_le_bytes());
        }
        Self::from_le_bytes(out_bytes)
    }

    #[inline]
    fn zip_f64x2(self, rhs: Self, mut op: impl FnMut(f64, f64) -> f64) -> Self {
        let a_bytes = self.to_le_bytes();
        let b_bytes = rhs.to_le_bytes();
        let mut out_bytes = [0u8; 16];

        for ((a, b), dst) in a_bytes.chunks_exact(8).zip(b_bytes.chunks_exact(8)).zip(out_bytes.chunks_exact_mut(8)) {
            let a_lane = f64::from_bits(u64::from_le_bytes([a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]]));
            let b_lane = f64::from_bits(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]));
            dst.copy_from_slice(&op(a_lane, b_lane).to_bits().to_le_bytes());
        }

        Self::from_le_bytes(out_bytes)
    }

    #[doc(alias = "v128.any_true")]
    pub fn v128_any_true(self) -> bool {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return wasm::v128_any_true(self.to_wasm_v128());
        self.0 != 0
    }

    #[doc(alias = "v128.not")]
    pub fn v128_not(self) -> Self {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return Self::from_wasm_v128(wasm::v128_not(self.to_wasm_v128()));
        Self(!self.0)
    }

    #[doc(alias = "v128.and")]
    pub fn v128_and(self, rhs: Self) -> Self {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return Self::from_wasm_v128(wasm::v128_and(self.to_wasm_v128(), rhs.to_wasm_v128()));
        Self(self.0 & rhs.0)
    }

    #[doc(alias = "v128.andnot")]
    pub fn v128_andnot(self, rhs: Self) -> Self {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return Self::from_wasm_v128(wasm::v128_andnot(self.to_wasm_v128(), rhs.to_wasm_v128()));
        Self(self.0 & !rhs.0)
    }

    #[doc(alias = "v128.or")]
    pub fn v128_or(self, rhs: Self) -> Self {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return Self::from_wasm_v128(wasm::v128_or(self.to_wasm_v128(), rhs.to_wasm_v128()));
        Self(self.0 | rhs.0)
    }

    #[doc(alias = "v128.xor")]
    pub fn v128_xor(self, rhs: Self) -> Self {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return Self::from_wasm_v128(wasm::v128_xor(self.to_wasm_v128(), rhs.to_wasm_v128()));
        Self(self.0 ^ rhs.0)
    }

    #[doc(alias = "v128.bitselect")]
    pub fn v128_bitselect(v1: Self, v2: Self, c: Self) -> Self {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return Self::from_wasm_v128(wasm::v128_bitselect(v1.to_wasm_v128(), v2.to_wasm_v128(), c.to_wasm_v128()));
        Self((v1.0 & c.0) | (v2.0 & !c.0))
    }

    #[doc(alias = "v128.load8x8_s")]
    pub const fn v128_load8x8_s(src: [u8; 8]) -> Self {
        Self::from_i16x8([
            src[0] as i8 as i16,
            src[1] as i8 as i16,
            src[2] as i8 as i16,
            src[3] as i8 as i16,
            src[4] as i8 as i16,
            src[5] as i8 as i16,
            src[6] as i8 as i16,
            src[7] as i8 as i16,
        ])
    }

    #[doc(alias = "v128.load8x8_u")]
    pub const fn v128_load8x8_u(src: [u8; 8]) -> Self {
        Self::from_u16x8([
            src[0] as u16,
            src[1] as u16,
            src[2] as u16,
            src[3] as u16,
            src[4] as u16,
            src[5] as u16,
            src[6] as u16,
            src[7] as u16,
        ])
    }

    #[doc(alias = "v128.load16x4_s")]
    pub const fn v128_load16x4_s(src: [u8; 8]) -> Self {
        Self::from_i32x4([
            i16::from_le_bytes([src[0], src[1]]) as i32,
            i16::from_le_bytes([src[2], src[3]]) as i32,
            i16::from_le_bytes([src[4], src[5]]) as i32,
            i16::from_le_bytes([src[6], src[7]]) as i32,
        ])
    }

    #[doc(alias = "v128.load16x4_u")]
    pub const fn v128_load16x4_u(src: [u8; 8]) -> Self {
        Self::from_u32x4([
            u16::from_le_bytes([src[0], src[1]]) as u32,
            u16::from_le_bytes([src[2], src[3]]) as u32,
            u16::from_le_bytes([src[4], src[5]]) as u32,
            u16::from_le_bytes([src[6], src[7]]) as u32,
        ])
    }

    #[doc(alias = "v128.load32x2_s")]
    pub const fn v128_load32x2_s(src: [u8; 8]) -> Self {
        Self::from_i64x2([
            i32::from_le_bytes([src[0], src[1], src[2], src[3]]) as i64,
            i32::from_le_bytes([src[4], src[5], src[6], src[7]]) as i64,
        ])
    }

    #[doc(alias = "v128.load32x2_u")]
    pub const fn v128_load32x2_u(src: [u8; 8]) -> Self {
        Self::from_u64x2([
            u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as u64,
            u32::from_le_bytes([src[4], src[5], src[6], src[7]]) as u64,
        ])
    }

    #[doc(alias = "i8x16.swizzle")]
    pub fn i8x16_swizzle(self, s: Self) -> Self {
        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        return Self::from_wasm_v128(wasm::i8x16_swizzle(self.to_wasm_v128(), s.to_wasm_v128()));

        let a = self.to_le_bytes();
        let idx = s.to_le_bytes();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            let j = idx[i];
            let lane = a[(j & 0x0f) as usize];
            out[i] = if j < 16 { lane } else { 0 };
            i += 1;
        }
        Self::from_le_bytes(out)
    }

    #[doc(alias = "i8x16.shuffle")]
    pub fn i8x16_shuffle(a: Self, b: Self, idx: [u8; 16]) -> Self {
        let mut src = [0u8; 32];
        src[..16].copy_from_slice(&a.to_le_bytes());
        src[16..].copy_from_slice(&b.to_le_bytes());
        let mut out = [0u8; 16];
        for i in 0..16 {
            out[i] = src[(idx[i] & 31) as usize];
        }
        Self::from_le_bytes(out)
    }

    pub fn splat_i8(src: i8) -> Self {
        Self::from_le_bytes([src as u8; 16])
    }

    #[doc(alias = "i8x16.replace_lane")]
    pub fn i8x16_replace_lane(self, lane: u8, value: i8) -> Self {
        self.replace_lane_bytes::<1>(lane, [value as u8], 16)
    }

    #[doc(alias = "i16x8.replace_lane")]
    pub fn i16x8_replace_lane(self, lane: u8, value: i16) -> Self {
        self.replace_lane_bytes::<2>(lane, value.to_le_bytes(), 8)
    }

    #[doc(alias = "i32x4.replace_lane")]
    pub fn i32x4_replace_lane(self, lane: u8, value: i32) -> Self {
        self.replace_lane_bytes::<4>(lane, value.to_le_bytes(), 4)
    }

    #[doc(alias = "i64x2.replace_lane")]
    pub fn i64x2_replace_lane(self, lane: u8, value: i64) -> Self {
        self.replace_lane_bytes::<8>(lane, value.to_le_bytes(), 2)
    }

    #[doc(alias = "f32x4.replace_lane")]
    pub fn f32x4_replace_lane(self, lane: u8, value: f32) -> Self {
        self.replace_lane_bytes::<4>(lane, value.to_bits().to_le_bytes(), 4)
    }

    #[doc(alias = "f64x2.replace_lane")]
    pub fn f64x2_replace_lane(self, lane: u8, value: f64) -> Self {
        self.replace_lane_bytes::<8>(lane, value.to_bits().to_le_bytes(), 2)
    }

    #[doc(alias = "i8x16.all_true")]
    pub fn i8x16_all_true(self) -> bool {
        for byte in self.to_le_bytes() {
            if byte == 0 {
                return false;
            }
        }
        true
    }

    #[doc(alias = "i16x8.all_true")]
    pub fn i16x8_all_true(self) -> bool {
        let bytes = self.to_le_bytes();
        for lane in bytes.chunks_exact(2) {
            if u16::from_le_bytes([lane[0], lane[1]]) == 0 {
                return false;
            }
        }
        true
    }

    #[doc(alias = "i32x4.all_true")]
    pub fn i32x4_all_true(self) -> bool {
        let bytes = self.to_le_bytes();
        for lane in bytes.chunks_exact(4) {
            if u32::from_le_bytes([lane[0], lane[1], lane[2], lane[3]]) == 0 {
                return false;
            }
        }
        true
    }

    #[doc(alias = "i64x2.all_true")]
    pub fn i64x2_all_true(self) -> bool {
        let bytes = self.to_le_bytes();
        for lane in bytes.chunks_exact(8) {
            if u64::from_le_bytes([lane[0], lane[1], lane[2], lane[3], lane[4], lane[5], lane[6], lane[7]]) == 0 {
                return false;
            }
        }
        true
    }

    #[doc(alias = "i8x16.bitmask")]
    pub fn i8x16_bitmask(self) -> u32 {
        let bytes = self.to_le_bytes();
        let mut mask = 0u32;
        for (i, byte) in bytes.into_iter().enumerate() {
            if (byte & 0x80) != 0 {
                mask |= 1u32 << i;
            }
        }
        mask
    }

    #[doc(alias = "i16x8.bitmask")]
    pub fn i16x8_bitmask(self) -> u32 {
        let bytes = self.to_le_bytes();
        let mut mask = 0u32;
        for (i, lane) in bytes.chunks_exact(2).enumerate() {
            if (lane[1] & 0x80) != 0 {
                mask |= 1u32 << i;
            }
        }
        mask
    }

    #[doc(alias = "i32x4.bitmask")]
    pub fn i32x4_bitmask(self) -> u32 {
        let bytes = self.to_le_bytes();
        let mut mask = 0u32;
        for (i, lane) in bytes.chunks_exact(4).enumerate() {
            if (lane[3] & 0x80) != 0 {
                mask |= 1u32 << i;
            }
        }
        mask
    }

    #[doc(alias = "i64x2.bitmask")]
    pub fn i64x2_bitmask(self) -> u32 {
        let x = u128::from_le_bytes(self.to_le_bytes());
        (((x >> 63) & 1) as u32) | ((((x >> 127) & 1) as u32) << 1)
    }

    #[doc(alias = "i8x16.popcnt")]
    pub fn i8x16_popcnt(self) -> Self {
        let lanes = self.to_le_bytes();
        let mut out = [0u8; 16];
        for (dst, lane) in out.iter_mut().zip(lanes) {
            *dst = lane.count_ones() as u8;
        }
        Self::from_le_bytes(out)
    }

    simd_shift_left!(i8x16_shl, "i8x16.shl", i8, 16, as_i8x16, from_i8x16, 7);
    simd_shift_left!(i16x8_shl, "i16x8.shl", i16, 8, as_i16x8, from_i16x8, 15);
    simd_shift_left!(i32x4_shl, "i32x4.shl", i32, 4, as_i32x4, from_i32x4, 31);
    simd_shift_left!(i64x2_shl, "i64x2.shl", i64, 2, as_i64x2, from_i64x2, 63);

    simd_shift_right!(i8x16_shr_s, "i8x16.shr_s", i8, 16, as_i8x16, from_i8x16, 7);
    simd_shift_right!(i16x8_shr_s, "i16x8.shr_s", i16, 8, as_i16x8, from_i16x8, 15);
    simd_shift_right!(i32x4_shr_s, "i32x4.shr_s", i32, 4, as_i32x4, from_i32x4, 31);
    simd_shift_right!(i64x2_shr_s, "i64x2.shr_s", i64, 2, as_i64x2, from_i64x2, 63);

    simd_shift_right!(i8x16_shr_u, "i8x16.shr_u", u8, 16, as_u8x16, from_u8x16, 7);
    simd_shift_right!(i16x8_shr_u, "i16x8.shr_u", u16, 8, as_u16x8, from_u16x8, 15);
    simd_shift_right!(i32x4_shr_u, "i32x4.shr_u", u32, 4, as_u32x4, from_u32x4, 31);
    simd_shift_right!(i64x2_shr_u, "i64x2.shr_u", u64, 2, as_u64x2, from_u64x2, 63);

    simd_wrapping_binop!(i8x16_add, "i8x16.add", i8x16_add, i8, 16, as_i8x16, from_i8x16, wrapping_add);
    simd_wrapping_binop!(i16x8_add, "i16x8.add", i16x8_add, i16, 8, as_i16x8, from_i16x8, wrapping_add);
    simd_wrapping_binop!(i32x4_add, "i32x4.add", i32x4_add, i32, 4, as_i32x4, from_i32x4, wrapping_add);
    simd_wrapping_binop!(i64x2_add, "i64x2.add", i64x2_add, i64, 2, as_i64x2, from_i64x2, wrapping_add);
    simd_wrapping_binop!(i8x16_sub, "i8x16.sub", i8x16_sub, i8, 16, as_i8x16, from_i8x16, wrapping_sub);
    simd_wrapping_binop!(i16x8_sub, "i16x8.sub", i16x8_sub, i16, 8, as_i16x8, from_i16x8, wrapping_sub);
    simd_wrapping_binop!(i32x4_sub, "i32x4.sub", i32x4_sub, i32, 4, as_i32x4, from_i32x4, wrapping_sub);
    simd_wrapping_binop!(i64x2_sub, "i64x2.sub", i64x2_sub, i64, 2, as_i64x2, from_i64x2, wrapping_sub);
    simd_wrapping_binop!(i16x8_mul, "i16x8.mul", i16x8_mul, i16, 8, as_i16x8, from_i16x8, wrapping_mul);
    simd_wrapping_binop!(i32x4_mul, "i32x4.mul", i32x4_mul, i32, 4, as_i32x4, from_i32x4, wrapping_mul);
    simd_wrapping_binop!(i64x2_mul, "i64x2.mul", i64x2_mul, i64, 2, as_i64x2, from_i64x2, wrapping_mul);

    simd_sat_binop!(i8x16_add_sat_s, "i8x16.add_sat_s", i8x16_add_sat, i8, 16, as_i8x16, from_i8x16, saturating_add);
    simd_sat_binop!(i16x8_add_sat_s, "i16x8.add_sat_s", i16x8_add_sat, i16, 8, as_i16x8, from_i16x8, saturating_add);
    simd_sat_binop!(i8x16_add_sat_u, "i8x16.add_sat_u", u8x16_add_sat, u8, 16, as_u8x16, from_u8x16, saturating_add);
    simd_sat_binop!(i16x8_add_sat_u, "i16x8.add_sat_u", u16x8_add_sat, u16, 8, as_u16x8, from_u16x8, saturating_add);
    simd_sat_binop!(i8x16_sub_sat_s, "i8x16.sub_sat_s", i8x16_sub_sat, i8, 16, as_i8x16, from_i8x16, saturating_sub);
    simd_sat_binop!(i16x8_sub_sat_s, "i16x8.sub_sat_s", i16x8_sub_sat, i16, 8, as_i16x8, from_i16x8, saturating_sub);
    simd_sat_binop!(i8x16_sub_sat_u, "i8x16.sub_sat_u", u8x16_sub_sat, u8, 16, as_u8x16, from_u8x16, saturating_sub);
    simd_sat_binop!(i16x8_sub_sat_u, "i16x8.sub_sat_u", u16x8_sub_sat, u16, 8, as_u16x8, from_u16x8, saturating_sub);

    simd_avgr_u!(i8x16_avgr_u, "i8x16.avgr_u", u8x16_avgr, u8, u16, 16, as_u8x16, from_u8x16);
    simd_avgr_u!(i16x8_avgr_u, "i16x8.avgr_u", u16x8_avgr, u16, u32, 8, as_u16x8, from_u16x8);

    #[doc(alias = "i8x16.narrow_i16x8_s")]
    pub fn i8x16_narrow_i16x8_s(a: Self, b: Self) -> Self {
        let av = a.as_i16x8();
        let bv = b.as_i16x8();
        let mut out = [0i8; 16];
        let (lo, hi) = out.split_at_mut(8);
        for ((dst_lo, dst_hi), (a_lane, b_lane)) in lo.iter_mut().zip(hi.iter_mut()).zip(av.into_iter().zip(bv)) {
            *dst_lo = saturate_i16_to_i8(a_lane);
            *dst_hi = saturate_i16_to_i8(b_lane);
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i8x16.narrow_i16x8_u")]
    pub fn i8x16_narrow_i16x8_u(a: Self, b: Self) -> Self {
        let av = a.as_i16x8();
        let bv = b.as_i16x8();
        let mut out = [0u8; 16];
        let (lo, hi) = out.split_at_mut(8);
        for ((dst_lo, dst_hi), (a_lane, b_lane)) in lo.iter_mut().zip(hi.iter_mut()).zip(av.into_iter().zip(bv)) {
            *dst_lo = saturate_i16_to_u8(a_lane);
            *dst_hi = saturate_i16_to_u8(b_lane);
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.narrow_i32x4_s")]
    pub fn i16x8_narrow_i32x4_s(a: Self, b: Self) -> Self {
        let av = a.as_i32x4();
        let bv = b.as_i32x4();
        let mut out = [0i16; 8];
        let (lo, hi) = out.split_at_mut(4);
        for ((dst_lo, dst_hi), (a_lane, b_lane)) in lo.iter_mut().zip(hi.iter_mut()).zip(av.into_iter().zip(bv)) {
            *dst_lo = saturate_i32_to_i16(a_lane);
            *dst_hi = saturate_i32_to_i16(b_lane);
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i16x8.narrow_i32x4_u")]
    pub fn i16x8_narrow_i32x4_u(a: Self, b: Self) -> Self {
        let av = a.as_i32x4();
        let bv = b.as_i32x4();
        let mut out = [0u16; 8];
        let (lo, hi) = out.split_at_mut(4);
        for ((dst_lo, dst_hi), (a_lane, b_lane)) in lo.iter_mut().zip(hi.iter_mut()).zip(av.into_iter().zip(bv)) {
            *dst_lo = saturate_i32_to_u16(a_lane);
            *dst_hi = saturate_i32_to_u16(b_lane);
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i16x8.extadd_pairwise_i8x16_s")]
    pub fn i16x8_extadd_pairwise_i8x16_s(self) -> Self {
        let lanes = self.as_i8x16();
        let mut out = [0i16; 8];
        for (dst, pair) in out.iter_mut().zip(lanes.chunks_exact(2)) {
            *dst = pair[0] as i16 + pair[1] as i16;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i16x8.extadd_pairwise_i8x16_u")]
    pub fn i16x8_extadd_pairwise_i8x16_u(self) -> Self {
        let lanes = self.as_u8x16();
        let mut out = [0u16; 8];
        for (dst, pair) in out.iter_mut().zip(lanes.chunks_exact(2)) {
            *dst = pair[0] as u16 + pair[1] as u16;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i32x4.extadd_pairwise_i16x8_s")]
    pub fn i32x4_extadd_pairwise_i16x8_s(self) -> Self {
        let lanes = self.as_i16x8();
        let mut out = [0i32; 4];
        for (dst, pair) in out.iter_mut().zip(lanes.chunks_exact(2)) {
            *dst = pair[0] as i32 + pair[1] as i32;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i32x4.extadd_pairwise_i16x8_u")]
    pub fn i32x4_extadd_pairwise_i16x8_u(self) -> Self {
        let lanes = self.as_u16x8();
        let mut out = [0u32; 4];
        for (dst, pair) in out.iter_mut().zip(lanes.chunks_exact(2)) {
            *dst = pair[0] as u32 + pair[1] as u32;
        }
        Self::from_u32x4(out)
    }

    simd_extend_cast!(i16x8_extend_low_i8x16_s, "i16x8.extend_low_i8x16_s", as_i8x16, from_i16x8, i16, 8, 0);
    simd_extend_cast!(i16x8_extend_low_i8x16_u, "i16x8.extend_low_i8x16_u", as_u8x16, from_u16x8, u16, 8, 0);
    simd_extend_cast!(i16x8_extend_high_i8x16_s, "i16x8.extend_high_i8x16_s", as_i8x16, from_i16x8, i16, 8, 8);
    simd_extend_cast!(i16x8_extend_high_i8x16_u, "i16x8.extend_high_i8x16_u", as_u8x16, from_u16x8, u16, 8, 8);
    simd_extend_cast!(i32x4_extend_low_i16x8_s, "i32x4.extend_low_i16x8_s", as_i16x8, from_i32x4, i32, 4, 0);
    simd_extend_cast!(i32x4_extend_low_i16x8_u, "i32x4.extend_low_i16x8_u", as_u16x8, from_u32x4, u32, 4, 0);
    simd_extend_cast!(i32x4_extend_high_i16x8_s, "i32x4.extend_high_i16x8_s", as_i16x8, from_i32x4, i32, 4, 4);
    simd_extend_cast!(i32x4_extend_high_i16x8_u, "i32x4.extend_high_i16x8_u", as_u16x8, from_u32x4, u32, 4, 4);
    simd_extend_cast!(i64x2_extend_low_i32x4_s, "i64x2.extend_low_i32x4_s", as_i32x4, from_i64x2, i64, 2, 0);
    simd_extend_cast!(i64x2_extend_low_i32x4_u, "i64x2.extend_low_i32x4_u", as_u32x4, from_u64x2, u64, 2, 0);
    simd_extend_cast!(i64x2_extend_high_i32x4_s, "i64x2.extend_high_i32x4_s", as_i32x4, from_i64x2, i64, 2, 2);
    simd_extend_cast!(i64x2_extend_high_i32x4_u, "i64x2.extend_high_i32x4_u", as_u32x4, from_u64x2, u64, 2, 2);

    simd_extmul_signed!(i16x8_extmul_low_i8x16_s, "i16x8.extmul_low_i8x16_s", as_i8x16, from_i16x8, i16, 8, 0);
    simd_extmul_unsigned!(i16x8_extmul_low_i8x16_u, "i16x8.extmul_low_i8x16_u", as_u8x16, from_u16x8, u16, 8, 0);
    simd_extmul_signed!(i16x8_extmul_high_i8x16_s, "i16x8.extmul_high_i8x16_s", as_i8x16, from_i16x8, i16, 8, 8);
    simd_extmul_unsigned!(i16x8_extmul_high_i8x16_u, "i16x8.extmul_high_i8x16_u", as_u8x16, from_u16x8, u16, 8, 8);
    simd_extmul_signed!(i32x4_extmul_low_i16x8_s, "i32x4.extmul_low_i16x8_s", as_i16x8, from_i32x4, i32, 4, 0);
    simd_extmul_unsigned!(i32x4_extmul_low_i16x8_u, "i32x4.extmul_low_i16x8_u", as_u16x8, from_u32x4, u32, 4, 0);
    simd_extmul_signed!(i32x4_extmul_high_i16x8_s, "i32x4.extmul_high_i16x8_s", as_i16x8, from_i32x4, i32, 4, 4);
    simd_extmul_unsigned!(i32x4_extmul_high_i16x8_u, "i32x4.extmul_high_i16x8_u", as_u16x8, from_u32x4, u32, 4, 4);
    simd_extmul_signed!(i64x2_extmul_low_i32x4_s, "i64x2.extmul_low_i32x4_s", as_i32x4, from_i64x2, i64, 2, 0);
    simd_extmul_unsigned!(i64x2_extmul_low_i32x4_u, "i64x2.extmul_low_i32x4_u", as_u32x4, from_u64x2, u64, 2, 0);
    simd_extmul_signed!(i64x2_extmul_high_i32x4_s, "i64x2.extmul_high_i32x4_s", as_i32x4, from_i64x2, i64, 2, 2);
    simd_extmul_unsigned!(i64x2_extmul_high_i32x4_u, "i64x2.extmul_high_i32x4_u", as_u32x4, from_u64x2, u64, 2, 2);

    #[doc(alias = "i16x8.q15mulr_sat_s")]
    pub fn i16x8_q15mulr_sat_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            let r = ((lhs as i32 * rhs as i32) + (1 << 14)) >> 15; // 2^14: Q15 rounding
            *dst = if r > i16::MAX as i32 {
                i16::MAX
            } else if r < i16::MIN as i32 {
                i16::MIN
            } else {
                r as i16
            };
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.dot_i16x8_s")]
    pub fn i32x4_dot_i16x8_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i32; 4];
        for (dst, (a_pair, b_pair)) in out.iter_mut().zip(a.chunks_exact(2).zip(b.chunks_exact(2))) {
            *dst = (a_pair[0] as i32)
                .wrapping_mul(b_pair[0] as i32)
                .wrapping_add((a_pair[1] as i32).wrapping_mul(b_pair[1] as i32));
        }
        Self::from_i32x4(out)
    }

    simd_cmp_mask!(i8x16_eq, "i8x16.eq", i8x16_eq, i8, 16, as_i8x16, from_i8x16, ==);
    simd_cmp_mask!(i16x8_eq, "i16x8.eq", i16x8_eq, i16, 8, as_i16x8, from_i16x8, ==);
    simd_cmp_mask!(i32x4_eq, "i32x4.eq", i32x4_eq, i32, 4, as_i32x4, from_i32x4, ==);
    simd_cmp_mask!(i64x2_eq, "i64x2.eq", i64x2_eq, i64, 2, as_i64x2, from_i64x2, ==);
    simd_cmp_mask!(i8x16_ne, "i8x16.ne", i8x16_ne, i8, 16, as_i8x16, from_i8x16, !=);
    simd_cmp_mask!(i16x8_ne, "i16x8.ne", i16x8_ne, i16, 8, as_i16x8, from_i16x8, !=);
    simd_cmp_mask!(i32x4_ne, "i32x4.ne", i32x4_ne, i32, 4, as_i32x4, from_i32x4, !=);
    simd_cmp_mask!(i64x2_ne, "i64x2.ne", i64x2_ne, i64, 2, as_i64x2, from_i64x2, !=);
    simd_cmp_mask!(i8x16_lt_s, "i8x16.lt_s", i8x16_lt, i8, 16, as_i8x16, from_i8x16, <);
    simd_cmp_mask!(i16x8_lt_s, "i16x8.lt_s", i16x8_lt, i16, 8, as_i16x8, from_i16x8, <);
    simd_cmp_mask!(i32x4_lt_s, "i32x4.lt_s", i32x4_lt, i32, 4, as_i32x4, from_i32x4, <);
    simd_cmp_mask!(i64x2_lt_s, "i64x2.lt_s", i64x2_lt, i64, 2, as_i64x2, from_i64x2, <);
    simd_cmp_mask!(i8x16_lt_u, "i8x16.lt_u", u8x16_lt, i8, 16, as_u8x16, from_i8x16, <);
    simd_cmp_mask!(i16x8_lt_u, "i16x8.lt_u", u16x8_lt, i16, 8, as_u16x8, from_i16x8, <);
    simd_cmp_mask!(i32x4_lt_u, "i32x4.lt_u", u32x4_lt, i32, 4, as_u32x4, from_i32x4, <);

    simd_cmp_delegate!(i8x16_gt_s, "i8x16.gt_s", i8x16_lt_s);
    simd_cmp_delegate!(i16x8_gt_s, "i16x8.gt_s", i16x8_lt_s);
    simd_cmp_delegate!(i32x4_gt_s, "i32x4.gt_s", i32x4_lt_s);
    simd_cmp_delegate!(i64x2_gt_s, "i64x2.gt_s", i64x2_lt_s);
    simd_cmp_delegate!(i8x16_gt_u, "i8x16.gt_u", i8x16_lt_u);
    simd_cmp_delegate!(i16x8_gt_u, "i16x8.gt_u", i16x8_lt_u);
    simd_cmp_delegate!(i32x4_gt_u, "i32x4.gt_u", i32x4_lt_u);
    simd_cmp_delegate!(i8x16_le_s, "i8x16.le_s", i8x16_ge_s);
    simd_cmp_delegate!(i16x8_le_s, "i16x8.le_s", i16x8_ge_s);
    simd_cmp_delegate!(i32x4_le_s, "i32x4.le_s", i32x4_ge_s);
    simd_cmp_delegate!(i64x2_le_s, "i64x2.le_s", i64x2_ge_s);
    simd_cmp_delegate!(i8x16_le_u, "i8x16.le_u", i8x16_ge_u);
    simd_cmp_delegate!(i16x8_le_u, "i16x8.le_u", i16x8_ge_u);
    simd_cmp_delegate!(i32x4_le_u, "i32x4.le_u", i32x4_ge_u);

    simd_cmp_mask!(i8x16_ge_s, "i8x16.ge_s", i8x16_ge, i8, 16, as_i8x16, from_i8x16, >=);
    simd_cmp_mask!(i16x8_ge_s, "i16x8.ge_s", i16x8_ge, i16, 8, as_i16x8, from_i16x8, >=);
    simd_cmp_mask!(i32x4_ge_s, "i32x4.ge_s", i32x4_ge, i32, 4, as_i32x4, from_i32x4, >=);
    simd_cmp_mask!(i64x2_ge_s, "i64x2.ge_s", i64x2_ge, i64, 2, as_i64x2, from_i64x2, >=);
    simd_cmp_mask!(i8x16_ge_u, "i8x16.ge_u", u8x16_ge, i8, 16, as_u8x16, from_i8x16, >=);
    simd_cmp_mask!(i16x8_ge_u, "i16x8.ge_u", u16x8_ge, i16, 8, as_u16x8, from_i16x8, >=);
    simd_cmp_mask!(i32x4_ge_u, "i32x4.ge_u", u32x4_ge, i32, 4, as_u32x4, from_i32x4, >=);

    simd_abs_const!(i8x16_abs, "i8x16.abs", i8, 16, as_i8x16, from_i8x16);
    simd_abs_const!(i16x8_abs, "i16x8.abs", i16, 8, as_i16x8, from_i16x8);
    simd_abs_const!(i32x4_abs, "i32x4.abs", i32, 4, as_i32x4, from_i32x4);
    simd_abs_const!(i64x2_abs, "i64x2.abs", i64, 2, as_i64x2, from_i64x2);

    simd_neg!(i8x16_neg, "i8x16.neg", i8x16_neg, i8, 16, as_i8x16, from_i8x16);
    simd_neg!(i16x8_neg, "i16x8.neg", i16x8_neg, i16, 8, as_i16x8, from_i16x8);
    simd_neg!(i32x4_neg, "i32x4.neg", i32x4_neg, i32, 4, as_i32x4, from_i32x4);
    simd_neg!(i64x2_neg, "i64x2.neg", i64x2_neg, i64, 2, as_i64x2, from_i64x2);

    simd_minmax!(i8x16_min_s, "i8x16.min_s", i8x16_min, i8, 16, as_i8x16, from_i8x16, <);
    simd_minmax!(i16x8_min_s, "i16x8.min_s", i16x8_min, i16, 8, as_i16x8, from_i16x8, <);
    simd_minmax!(i32x4_min_s, "i32x4.min_s", i32x4_min, i32, 4, as_i32x4, from_i32x4, <);
    simd_minmax!(i8x16_min_u, "i8x16.min_u", u8x16_min, u8, 16, as_u8x16, from_u8x16, <);
    simd_minmax!(i16x8_min_u, "i16x8.min_u", u16x8_min, u16, 8, as_u16x8, from_u16x8, <);
    simd_minmax!(i32x4_min_u, "i32x4.min_u", u32x4_min, u32, 4, as_u32x4, from_u32x4, <);
    simd_minmax!(i8x16_max_s, "i8x16.max_s", i8x16_max, i8, 16, as_i8x16, from_i8x16, >);
    simd_minmax!(i16x8_max_s, "i16x8.max_s", i16x8_max, i16, 8, as_i16x8, from_i16x8, >);
    simd_minmax!(i32x4_max_s, "i32x4.max_s", i32x4_max, i32, 4, as_i32x4, from_i32x4, >);
    simd_minmax!(i8x16_max_u, "i8x16.max_u", u8x16_max, u8, 16, as_u8x16, from_u8x16, >);
    simd_minmax!(i16x8_max_u, "i16x8.max_u", u16x8_max, u16, 8, as_u16x8, from_u16x8, >);
    simd_minmax!(i32x4_max_u, "i32x4.max_u", u32x4_max, u32, 4, as_u32x4, from_u32x4, >);

    simd_cmp_mask_const!(f32x4_eq, "f32x4.eq", i32, 4, as_f32x4, from_i32x4, ==);
    simd_cmp_mask_const!(f64x2_eq, "f64x2.eq", i64, 2, as_f64x2, from_i64x2, ==);
    simd_cmp_mask_const!(f32x4_ne, "f32x4.ne", i32, 4, as_f32x4, from_i32x4, !=);
    simd_cmp_mask_const!(f64x2_ne, "f64x2.ne", i64, 2, as_f64x2, from_i64x2, !=);
    simd_cmp_mask_const!(f32x4_lt, "f32x4.lt", i32, 4, as_f32x4, from_i32x4, <);
    simd_cmp_mask_const!(f64x2_lt, "f64x2.lt", i64, 2, as_f64x2, from_i64x2, <);

    #[doc(alias = "f32x4.gt")]
    pub fn f32x4_gt(self, rhs: Self) -> Self {
        rhs.f32x4_lt(self)
    }

    #[doc(alias = "f64x2.gt")]
    pub fn f64x2_gt(self, rhs: Self) -> Self {
        rhs.f64x2_lt(self)
    }

    simd_cmp_mask_const!(f32x4_le, "f32x4.le", i32, 4, as_f32x4, from_i32x4, <=);
    simd_cmp_mask_const!(f64x2_le, "f64x2.le", i64, 2, as_f64x2, from_i64x2, <=);
    simd_cmp_mask_const!(f32x4_ge, "f32x4.ge", i32, 4, as_f32x4, from_i32x4, >=);
    simd_cmp_mask_const!(f64x2_ge, "f64x2.ge", i64, 2, as_f64x2, from_i64x2, >=);

    simd_float_unary!(f32x4_ceil, "f32x4.ceil", map_f32x4, |x| canonicalize_simd_f32_nan(x.ceil()));
    simd_float_unary!(f64x2_ceil, "f64x2.ceil", map_f64x2, |x| canonicalize_simd_f64_nan(x.ceil()));
    simd_float_unary!(f32x4_floor, "f32x4.floor", map_f32x4, |x| canonicalize_simd_f32_nan(x.floor()));
    simd_float_unary!(f64x2_floor, "f64x2.floor", map_f64x2, |x| canonicalize_simd_f64_nan(x.floor()));
    simd_float_unary!(f32x4_trunc, "f32x4.trunc", map_f32x4, |x| canonicalize_simd_f32_nan(x.trunc()));
    simd_float_unary!(f64x2_trunc, "f64x2.trunc", map_f64x2, |x| canonicalize_simd_f64_nan(x.trunc()));
    simd_float_unary!(f32x4_nearest, "f32x4.nearest", map_f32x4, |x| canonicalize_simd_f32_nan(
        TinywasmFloatExt::tw_nearest(x)
    ));
    simd_float_unary!(f64x2_nearest, "f64x2.nearest", map_f64x2, |x| canonicalize_simd_f64_nan(
        TinywasmFloatExt::tw_nearest(x)
    ));
    simd_float_unary!(f32x4_abs, "f32x4.abs", map_f32x4, f32::abs);
    simd_float_unary!(f64x2_abs, "f64x2.abs", map_f64x2, f64::abs);
    simd_float_unary!(f32x4_neg, "f32x4.neg", map_f32x4, |x| -x);
    simd_float_unary!(f64x2_neg, "f64x2.neg", map_f64x2, |x| -x);
    simd_float_unary!(f32x4_sqrt, "f32x4.sqrt", map_f32x4, |x| canonicalize_simd_f32_nan(x.sqrt()));
    simd_float_unary!(f64x2_sqrt, "f64x2.sqrt", map_f64x2, |x| canonicalize_simd_f64_nan(x.sqrt()));

    simd_float_binary!(f32x4_add, "f32x4.add", zip_f32x4, |a, b| canonicalize_simd_f32_nan(a + b));
    simd_float_binary!(f64x2_add, "f64x2.add", zip_f64x2, |a, b| canonicalize_simd_f64_nan(a + b));
    simd_float_binary!(f32x4_sub, "f32x4.sub", zip_f32x4, |a, b| canonicalize_simd_f32_nan(a - b));
    simd_float_binary!(f64x2_sub, "f64x2.sub", zip_f64x2, |a, b| canonicalize_simd_f64_nan(a - b));
    simd_float_binary!(f32x4_mul, "f32x4.mul", zip_f32x4, |a, b| canonicalize_simd_f32_nan(a * b));
    simd_float_binary!(f64x2_mul, "f64x2.mul", zip_f64x2, |a, b| canonicalize_simd_f64_nan(a * b));
    simd_float_binary!(f32x4_div, "f32x4.div", zip_f32x4, |a, b| canonicalize_simd_f32_nan(a / b));
    simd_float_binary!(f64x2_div, "f64x2.div", zip_f64x2, |a, b| canonicalize_simd_f64_nan(a / b));
    simd_float_binary!(f32x4_min, "f32x4.min", zip_f32x4, TinywasmFloatExt::tw_minimum);
    simd_float_binary!(f64x2_min, "f64x2.min", zip_f64x2, TinywasmFloatExt::tw_minimum);
    simd_float_binary!(f32x4_max, "f32x4.max", zip_f32x4, TinywasmFloatExt::tw_maximum);
    simd_float_binary!(f64x2_max, "f64x2.max", zip_f64x2, TinywasmFloatExt::tw_maximum);
    simd_float_binary!(f32x4_pmin, "f32x4.pmin", zip_f32x4, |a, b| if b < a { b } else { a });
    simd_float_binary!(f64x2_pmin, "f64x2.pmin", zip_f64x2, |a, b| if b < a { b } else { a });
    simd_float_binary!(f32x4_pmax, "f32x4.pmax", zip_f32x4, |a, b| if b > a { b } else { a });
    simd_float_binary!(f64x2_pmax, "f64x2.pmax", zip_f64x2, |a, b| if b > a { b } else { a });

    #[doc(alias = "i32x4.trunc_sat_f32x4_s")]
    pub fn i32x4_trunc_sat_f32x4_s(self) -> Self {
        let v = self.as_f32x4();
        Self::from_i32x4([
            trunc_sat_f32_to_i32(v[0]),
            trunc_sat_f32_to_i32(v[1]),
            trunc_sat_f32_to_i32(v[2]),
            trunc_sat_f32_to_i32(v[3]),
        ])
    }

    #[doc(alias = "i32x4.trunc_sat_f32x4_u")]
    pub fn i32x4_trunc_sat_f32x4_u(self) -> Self {
        let v = self.as_f32x4();
        Self::from_u32x4([
            trunc_sat_f32_to_u32(v[0]),
            trunc_sat_f32_to_u32(v[1]),
            trunc_sat_f32_to_u32(v[2]),
            trunc_sat_f32_to_u32(v[3]),
        ])
    }

    #[doc(alias = "i32x4.trunc_sat_f64x2_s_zero")]
    pub fn i32x4_trunc_sat_f64x2_s_zero(self) -> Self {
        let v = self.as_f64x2();
        Self::from_i32x4([trunc_sat_f64_to_i32(v[0]), trunc_sat_f64_to_i32(v[1]), 0, 0])
    }

    #[doc(alias = "i32x4.trunc_sat_f64x2_u_zero")]
    pub fn i32x4_trunc_sat_f64x2_u_zero(self) -> Self {
        let v = self.as_f64x2();
        Self::from_u32x4([trunc_sat_f64_to_u32(v[0]), trunc_sat_f64_to_u32(v[1]), 0, 0])
    }

    #[doc(alias = "f32x4.convert_i32x4_s")]
    pub fn f32x4_convert_i32x4_s(self) -> Self {
        let v = self.as_i32x4();
        Self::from_f32x4([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32])
    }

    #[doc(alias = "f32x4.convert_i32x4_u")]
    pub fn f32x4_convert_i32x4_u(self) -> Self {
        let v = self.as_u32x4();
        Self::from_f32x4([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32])
    }

    #[doc(alias = "f64x2.convert_low_i32x4_s")]
    pub fn f64x2_convert_low_i32x4_s(self) -> Self {
        let v = self.as_i32x4();
        Self::from_f64x2([v[0] as f64, v[1] as f64])
    }

    #[doc(alias = "f64x2.convert_low_i32x4_u")]
    pub fn f64x2_convert_low_i32x4_u(self) -> Self {
        let v = self.as_u32x4();
        Self::from_f64x2([v[0] as f64, v[1] as f64])
    }

    #[doc(alias = "f32x4.demote_f64x2_zero")]
    pub fn f32x4_demote_f64x2_zero(self) -> Self {
        let v = self.as_f64x2();
        Self::from_f32x4([v[0] as f32, v[1] as f32, 0.0, 0.0])
    }

    #[doc(alias = "f64x2.promote_low_f32x4")]
    pub fn f64x2_promote_low_f32x4(self) -> Self {
        let v = self.as_f32x4();
        Self::from_f64x2([v[0] as f64, v[1] as f64])
    }

    pub fn splat_i16(src: i16) -> Self {
        Self::from_i16x8([src; 8])
    }

    pub fn splat_i32(src: i32) -> Self {
        Self::from_i32x4([src; 4])
    }

    pub fn splat_i64(src: i64) -> Self {
        Self::from_i64x2([src; 2])
    }

    pub fn splat_f32(src: f32) -> Self {
        Self::splat_i32(src.to_bits() as i32)
    }

    pub fn splat_f64(src: f64) -> Self {
        Self::splat_i64(src.to_bits() as i64)
    }

    pub fn extract_lane_i8(self, lane: u8) -> i8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane] as i8
    }

    pub fn extract_lane_u8(self, lane: u8) -> u8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane]
    }

    pub fn extract_lane_i16(self, lane: u8) -> i16 {
        i16::from_le_bytes(self.extract_lane_bytes::<2>(lane, 8))
    }

    pub fn extract_lane_u16(self, lane: u8) -> u16 {
        u16::from_le_bytes(self.extract_lane_bytes::<2>(lane, 8))
    }

    pub fn extract_lane_i32(self, lane: u8) -> i32 {
        i32::from_le_bytes(self.extract_lane_bytes::<4>(lane, 4))
    }

    pub fn extract_lane_i64(self, lane: u8) -> i64 {
        i64::from_le_bytes(self.extract_lane_bytes::<8>(lane, 2))
    }

    pub fn extract_lane_f32(self, lane: u8) -> f32 {
        f32::from_bits(self.extract_lane_i32(lane) as u32)
    }

    pub fn extract_lane_f64(self, lane: u8) -> f64 {
        f64::from_bits(self.extract_lane_i64(lane) as u64)
    }

    fn extract_lane_bytes<const LANE_BYTES: usize>(self, lane: u8, lane_count: u8) -> [u8; LANE_BYTES] {
        debug_assert!(lane < lane_count);
        let bytes = self.to_le_bytes();
        let start = lane as usize * LANE_BYTES;
        let mut out = [0u8; LANE_BYTES];
        out.copy_from_slice(&bytes[start..start + LANE_BYTES]);
        out
    }

    fn replace_lane_bytes<const LANE_BYTES: usize>(self, lane: u8, value: [u8; LANE_BYTES], lane_count: u8) -> Self {
        debug_assert!(lane < lane_count);
        let mut bytes = self.to_le_bytes();
        let start = lane as usize * LANE_BYTES;
        bytes[start..start + LANE_BYTES].copy_from_slice(&value);
        Self::from_le_bytes(bytes)
    }
}

const fn canonicalize_simd_f32_nan(x: f32) -> f32 {
    #[cfg(feature = "canonicalize_nans")]
    if x.is_nan() {
        f32::NAN
    } else {
        x
    }
    #[cfg(not(feature = "canonicalize_nans"))]
    x
}

const fn canonicalize_simd_f64_nan(x: f64) -> f64 {
    #[cfg(feature = "canonicalize_nans")]
    if x.is_nan() {
        f64::NAN
    } else {
        x
    }
    #[cfg(not(feature = "canonicalize_nans"))]
    x
}

const fn saturate_i16_to_i8(x: i16) -> i8 {
    match x {
        v if v > i8::MAX as i16 => i8::MAX,
        v if v < i8::MIN as i16 => i8::MIN,
        v => v as i8,
    }
}

const fn saturate_i16_to_u8(x: i16) -> u8 {
    match x {
        v if v <= 0 => 0,
        v if v > u8::MAX as i16 => u8::MAX,
        v => v as u8,
    }
}

const fn saturate_i32_to_i16(x: i32) -> i16 {
    match x {
        v if v > i16::MAX as i32 => i16::MAX,
        v if v < i16::MIN as i32 => i16::MIN,
        v => v as i16,
    }
}

const fn saturate_i32_to_u16(x: i32) -> u16 {
    match x {
        v if v <= 0 => 0,
        v if v > u16::MAX as i32 => u16::MAX,
        v => v as u16,
    }
}

fn trunc_sat_f32_to_i32(v: f32) -> i32 {
    match v {
        x if x.is_nan() => 0,
        x if x <= i32::MIN as f32 - (1 << 8) as f32 => i32::MIN,
        x if x >= (i32::MAX as f32 + 1.0) => i32::MAX,
        x => x.trunc() as i32,
    }
}

fn trunc_sat_f32_to_u32(v: f32) -> u32 {
    match v {
        x if x.is_nan() || x <= -1.0_f32 => 0,
        x if x >= (u32::MAX as f32 + 1.0) => u32::MAX,
        x => x.trunc() as u32,
    }
}

fn trunc_sat_f64_to_i32(v: f64) -> i32 {
    match v {
        x if x.is_nan() => 0,
        x if x <= i32::MIN as f64 - 1.0_f64 => i32::MIN,
        x if x >= (i32::MAX as f64 + 1.0) => i32::MAX,
        x => x.trunc() as i32,
    }
}

fn trunc_sat_f64_to_u32(v: f64) -> u32 {
    match v {
        x if x.is_nan() || x <= -1.0_f64 => 0,
        x if x >= (u32::MAX as f64 + 1.0) => u32::MAX,
        x => x.trunc() as u32,
    }
}
