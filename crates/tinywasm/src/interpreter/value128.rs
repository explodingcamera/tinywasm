use super::num_helpers::TinywasmFloatExt;

#[cfg(not(feature = "std"))]
use super::no_std_floats::NoStdFloatExt;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Value128(i128);

impl Value128 {
    pub const fn from_le_bytes(bytes: [u8; 16]) -> Self {
        Self(i128::from_le_bytes(bytes))
    }

    pub const fn to_le_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    #[rustfmt::skip]
    pub const fn as_i8x16(self) -> [i8; 16] {
        let b = self.to_le_bytes();
        [b[0] as i8, b[1] as i8, b[2] as i8, b[3] as i8, b[4] as i8, b[5] as i8, b[6] as i8, b[7] as i8, b[8] as i8, b[9] as i8, b[10] as i8, b[11] as i8, b[12] as i8, b[13] as i8, b[14] as i8, b[15] as i8]
    }

    #[rustfmt::skip]
    pub const fn as_u8x16(self) -> [u8; 16] {
        self.to_le_bytes()
    }

    #[rustfmt::skip]
    pub const fn from_i8x16(x: [i8; 16]) -> Self {
        Self::from_le_bytes([x[0] as u8, x[1] as u8, x[2] as u8, x[3] as u8, x[4] as u8, x[5] as u8, x[6] as u8, x[7] as u8, x[8] as u8, x[9] as u8, x[10] as u8, x[11] as u8, x[12] as u8, x[13] as u8, x[14] as u8, x[15] as u8])
    }

    #[rustfmt::skip]
    pub const fn from_u8x16(x: [u8; 16]) -> Self {
        Self::from_le_bytes(x)
    }

    #[rustfmt::skip]
    pub const fn as_i16x8(self) -> [i16; 8] {
        let b = self.to_le_bytes();
        [i16::from_le_bytes([b[0], b[1]]), i16::from_le_bytes([b[2], b[3]]), i16::from_le_bytes([b[4], b[5]]), i16::from_le_bytes([b[6], b[7]]), i16::from_le_bytes([b[8], b[9]]), i16::from_le_bytes([b[10], b[11]]), i16::from_le_bytes([b[12], b[13]]), i16::from_le_bytes([b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_u16x8(self) -> [u16; 8] {
        let b = self.to_le_bytes();
        [u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]]), u16::from_le_bytes([b[4], b[5]]), u16::from_le_bytes([b[6], b[7]]), u16::from_le_bytes([b[8], b[9]]), u16::from_le_bytes([b[10], b[11]]), u16::from_le_bytes([b[12], b[13]]), u16::from_le_bytes([b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn from_i16x8(x: [i16; 8]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[4].to_le_bytes()[0], x[4].to_le_bytes()[1], x[5].to_le_bytes()[0], x[5].to_le_bytes()[1], x[6].to_le_bytes()[0], x[6].to_le_bytes()[1], x[7].to_le_bytes()[0], x[7].to_le_bytes()[1]])
    }

    #[rustfmt::skip]
    pub const fn from_u16x8(x: [u16; 8]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[4].to_le_bytes()[0], x[4].to_le_bytes()[1], x[5].to_le_bytes()[0], x[5].to_le_bytes()[1], x[6].to_le_bytes()[0], x[6].to_le_bytes()[1], x[7].to_le_bytes()[0], x[7].to_le_bytes()[1]])
    }

    #[rustfmt::skip]
    pub const fn as_i32x4(self) -> [i32; 4] {
        let b = self.to_le_bytes();
        [i32::from_le_bytes([b[0], b[1], b[2], b[3]]), i32::from_le_bytes([b[4], b[5], b[6], b[7]]), i32::from_le_bytes([b[8], b[9], b[10], b[11]]), i32::from_le_bytes([b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_u32x4(self) -> [u32; 4] {
        let b = self.to_le_bytes();
        [u32::from_le_bytes([b[0], b[1], b[2], b[3]]), u32::from_le_bytes([b[4], b[5], b[6], b[7]]), u32::from_le_bytes([b[8], b[9], b[10], b[11]]), u32::from_le_bytes([b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_f32x4(self) -> [f32; 4] {
        let b = self.to_le_bytes();
        [f32::from_bits(u32::from_le_bytes([b[0], b[1], b[2], b[3]])), f32::from_bits(u32::from_le_bytes([b[4], b[5], b[6], b[7]])), f32::from_bits(u32::from_le_bytes([b[8], b[9], b[10], b[11]])), f32::from_bits(u32::from_le_bytes([b[12], b[13], b[14], b[15]]))]
    }

    #[rustfmt::skip]
    pub const fn from_i32x4(x: [i32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[2].to_le_bytes()[2], x[2].to_le_bytes()[3], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[3].to_le_bytes()[2], x[3].to_le_bytes()[3]])
    }

    #[rustfmt::skip]
    pub const fn from_u32x4(x: [u32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[2].to_le_bytes()[0], x[2].to_le_bytes()[1], x[2].to_le_bytes()[2], x[2].to_le_bytes()[3], x[3].to_le_bytes()[0], x[3].to_le_bytes()[1], x[3].to_le_bytes()[2], x[3].to_le_bytes()[3]])
    }

    #[rustfmt::skip]
    pub const fn from_f32x4(x: [f32; 4]) -> Self {
        Self::from_le_bytes([x[0].to_bits().to_le_bytes()[0], x[0].to_bits().to_le_bytes()[1], x[0].to_bits().to_le_bytes()[2], x[0].to_bits().to_le_bytes()[3], x[1].to_bits().to_le_bytes()[0], x[1].to_bits().to_le_bytes()[1], x[1].to_bits().to_le_bytes()[2], x[1].to_bits().to_le_bytes()[3], x[2].to_bits().to_le_bytes()[0], x[2].to_bits().to_le_bytes()[1], x[2].to_bits().to_le_bytes()[2], x[2].to_bits().to_le_bytes()[3], x[3].to_bits().to_le_bytes()[0], x[3].to_bits().to_le_bytes()[1], x[3].to_bits().to_le_bytes()[2], x[3].to_bits().to_le_bytes()[3]])
    }

    #[rustfmt::skip]
    pub const fn as_i64x2(self) -> [i64; 2] {
        let b = self.to_le_bytes();
        [i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), i64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_u64x2(self) -> [u64; 2] {
        let b = self.to_le_bytes();
        [u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]])]
    }

    #[rustfmt::skip]
    pub const fn as_f64x2(self) -> [f64; 2] {
        let b = self.to_le_bytes();
        [f64::from_bits(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])), f64::from_bits(u64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]))]
    }

    #[rustfmt::skip]
    pub const fn from_i64x2(x: [i64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[0].to_le_bytes()[4], x[0].to_le_bytes()[5], x[0].to_le_bytes()[6], x[0].to_le_bytes()[7], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[1].to_le_bytes()[4], x[1].to_le_bytes()[5], x[1].to_le_bytes()[6], x[1].to_le_bytes()[7]])
    }

    #[rustfmt::skip]
    pub const fn from_u64x2(x: [u64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_le_bytes()[0], x[0].to_le_bytes()[1], x[0].to_le_bytes()[2], x[0].to_le_bytes()[3], x[0].to_le_bytes()[4], x[0].to_le_bytes()[5], x[0].to_le_bytes()[6], x[0].to_le_bytes()[7], x[1].to_le_bytes()[0], x[1].to_le_bytes()[1], x[1].to_le_bytes()[2], x[1].to_le_bytes()[3], x[1].to_le_bytes()[4], x[1].to_le_bytes()[5], x[1].to_le_bytes()[6], x[1].to_le_bytes()[7]])
    }

    #[rustfmt::skip]
    pub const fn from_f64x2(x: [f64; 2]) -> Self {
        Self::from_le_bytes([x[0].to_bits().to_le_bytes()[0], x[0].to_bits().to_le_bytes()[1], x[0].to_bits().to_le_bytes()[2], x[0].to_bits().to_le_bytes()[3], x[0].to_bits().to_le_bytes()[4], x[0].to_bits().to_le_bytes()[5], x[0].to_bits().to_le_bytes()[6], x[0].to_bits().to_le_bytes()[7], x[1].to_bits().to_le_bytes()[0], x[1].to_bits().to_le_bytes()[1], x[1].to_bits().to_le_bytes()[2], x[1].to_bits().to_le_bytes()[3], x[1].to_bits().to_le_bytes()[4], x[1].to_bits().to_le_bytes()[5], x[1].to_bits().to_le_bytes()[6], x[1].to_bits().to_le_bytes()[7]])
    }

    #[inline(always)]
    fn map_f32x4(self, mut op: impl FnMut(f32) -> f32) -> Self {
        let lanes = self.as_f32x4();
        Self::from_f32x4([op(lanes[0]), op(lanes[1]), op(lanes[2]), op(lanes[3])])
    }

    #[inline(always)]
    fn zip_f32x4(self, rhs: Self, mut op: impl FnMut(f32, f32) -> f32) -> Self {
        let a = self.as_f32x4();
        let b = rhs.as_f32x4();
        Self::from_f32x4([op(a[0], b[0]), op(a[1], b[1]), op(a[2], b[2]), op(a[3], b[3])])
    }

    #[inline(always)]
    fn map_f64x2(self, mut op: impl FnMut(f64) -> f64) -> Self {
        let lanes = self.as_f64x2();
        Self::from_f64x2([op(lanes[0]), op(lanes[1])])
    }

    #[inline(always)]
    fn zip_f64x2(self, rhs: Self, mut op: impl FnMut(f64, f64) -> f64) -> Self {
        let a = self.as_f64x2();
        let b = rhs.as_f64x2();
        Self::from_f64x2([op(a[0], b[0]), op(a[1], b[1])])
    }

    #[inline]
    pub const fn reduce_or(self) -> u8 {
        let mut result = 0u8;
        let bytes = self.to_le_bytes();
        let mut i = 0;
        while i < 16 {
            result |= bytes[i];
            i += 1;
        }
        result
    }

    #[doc(alias = "v128.any_true")]
    pub const fn v128_any_true(self) -> bool {
        self.reduce_or() != 0
    }

    #[doc(alias = "v128.not")]
    pub const fn v128_not(self) -> Self {
        Self(!self.0)
    }

    #[doc(alias = "v128.and")]
    pub const fn v128_and(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }

    #[doc(alias = "v128.andnot")]
    pub const fn v128_andnot(self, rhs: Self) -> Self {
        Self(self.0 & !rhs.0)
    }

    #[doc(alias = "v128.or")]
    pub const fn v128_or(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }

    #[doc(alias = "v128.xor")]
    pub const fn v128_xor(self, rhs: Self) -> Self {
        Self(self.0 ^ rhs.0)
    }

    #[doc(alias = "v128.bitselect")]
    pub const fn v128_bitselect(v1: Self, v2: Self, c: Self) -> Self {
        Self((v1.0 & c.0) | (v2.0 & !c.0))
    }

    pub const fn swizzle(self, s: Self) -> Self {
        self.i8x16_swizzle(s)
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
    pub const fn i8x16_swizzle(self, s: Self) -> Self {
        let a_bytes = self.to_le_bytes();
        let s_bytes = s.to_le_bytes();
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            let index = s_bytes[i] as usize;
            result_bytes[i] = if index < 16 { a_bytes[index] } else { 0 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    #[doc(alias = "i8x16.shuffle")]
    pub const fn i8x16_shuffle(a: Self, b: Self, idx: [u8; 16]) -> Self {
        let a_bytes = a.to_le_bytes();
        let b_bytes = b.to_le_bytes();
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            let index = idx[i] as usize;
            result_bytes[i] = if index < 16 { a_bytes[index] } else { b_bytes[index - 16] };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_8_i8(src: i8) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 8 {
            result_bytes[i * 2] = src as u8;
            result_bytes[i * 2 + 1] = if src < 0 { 0xFF } else { 0x00 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_8_u8(src: u8) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 8 {
            result_bytes[i * 2] = src;
            result_bytes[i * 2 + 1] = 0x00;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_4_i16(src: i16) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 4 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 4] = bytes[0];
            result_bytes[i * 4 + 1] = bytes[1];
            result_bytes[i * 4 + 2] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 4 + 3] = if src < 0 { 0xFF } else { 0x00 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_4_u16(src: u16) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 4 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 4] = bytes[0];
            result_bytes[i * 4 + 1] = bytes[1];
            result_bytes[i * 4 + 2] = 0x00;
            result_bytes[i * 4 + 3] = 0x00;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_2_i32(src: i32) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 2 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 8] = bytes[0];
            result_bytes[i * 8 + 1] = bytes[1];
            result_bytes[i * 8 + 2] = bytes[2];
            result_bytes[i * 8 + 3] = bytes[3];
            result_bytes[i * 8 + 4] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 8 + 5] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 8 + 6] = if src < 0 { 0xFF } else { 0x00 };
            result_bytes[i * 8 + 7] = if src < 0 { 0xFF } else { 0x00 };
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn extend_2_u32(src: u32) -> Self {
        let mut result_bytes = [0u8; 16];
        let mut i = 0;
        while i < 2 {
            let bytes = src.to_le_bytes();
            result_bytes[i * 8] = bytes[0];
            result_bytes[i * 8 + 1] = bytes[1];
            result_bytes[i * 8 + 2] = bytes[2];
            result_bytes[i * 8 + 3] = bytes[3];
            result_bytes[i * 8 + 4] = 0x00;
            result_bytes[i * 8 + 5] = 0x00;
            result_bytes[i * 8 + 6] = 0x00;
            result_bytes[i * 8 + 7] = 0x00;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_i8(src: i8) -> Self {
        let mut result_bytes = [0u8; 16];
        let byte = src as u8;
        let mut i = 0;
        while i < 16 {
            result_bytes[i] = byte;
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    #[doc(alias = "i8x16.replace_lane")]
    pub const fn i8x16_replace_lane(self, lane: u8, value: i8) -> Self {
        self.replace_lane_bytes::<1>(lane, [value as u8], 16)
    }

    #[doc(alias = "i16x8.replace_lane")]
    pub const fn i16x8_replace_lane(self, lane: u8, value: i16) -> Self {
        self.replace_lane_bytes::<2>(lane, value.to_le_bytes(), 8)
    }

    #[doc(alias = "i32x4.replace_lane")]
    pub const fn i32x4_replace_lane(self, lane: u8, value: i32) -> Self {
        self.replace_lane_bytes::<4>(lane, value.to_le_bytes(), 4)
    }

    #[doc(alias = "i64x2.replace_lane")]
    pub const fn i64x2_replace_lane(self, lane: u8, value: i64) -> Self {
        self.replace_lane_bytes::<8>(lane, value.to_le_bytes(), 2)
    }

    #[doc(alias = "f32x4.replace_lane")]
    pub const fn f32x4_replace_lane(self, lane: u8, value: f32) -> Self {
        self.replace_lane_bytes::<4>(lane, value.to_bits().to_le_bytes(), 4)
    }

    #[doc(alias = "f64x2.replace_lane")]
    pub const fn f64x2_replace_lane(self, lane: u8, value: f64) -> Self {
        self.replace_lane_bytes::<8>(lane, value.to_bits().to_le_bytes(), 2)
    }

    #[doc(alias = "i8x16.all_true")]
    pub const fn i8x16_all_true(self) -> bool {
        let lanes = self.as_i8x16();
        let mut i = 0;
        while i < 16 {
            if lanes[i] == 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    #[doc(alias = "i16x8.all_true")]
    pub const fn i16x8_all_true(self) -> bool {
        let lanes = self.as_i16x8();
        let mut i = 0;
        while i < 8 {
            if lanes[i] == 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    #[doc(alias = "i32x4.all_true")]
    pub const fn i32x4_all_true(self) -> bool {
        let lanes = self.as_i32x4();
        let mut i = 0;
        while i < 4 {
            if lanes[i] == 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    #[doc(alias = "i64x2.all_true")]
    pub const fn i64x2_all_true(self) -> bool {
        let lanes = self.as_i64x2();
        let mut i = 0;
        while i < 2 {
            if lanes[i] == 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    #[doc(alias = "i8x16.bitmask")]
    pub const fn i8x16_bitmask(self) -> u32 {
        let lanes = self.as_i8x16();
        let mut mask = 0u32;
        let mut i = 0;
        while i < 16 {
            mask |= ((lanes[i] < 0) as u32) << i;
            i += 1;
        }
        mask
    }

    #[doc(alias = "i16x8.bitmask")]
    pub const fn i16x8_bitmask(self) -> u32 {
        let lanes = self.as_i16x8();
        let mut mask = 0u32;
        let mut i = 0;
        while i < 8 {
            mask |= ((lanes[i] < 0) as u32) << i;
            i += 1;
        }
        mask
    }

    #[doc(alias = "i32x4.bitmask")]
    pub const fn i32x4_bitmask(self) -> u32 {
        let lanes = self.as_i32x4();
        let mut mask = 0u32;
        let mut i = 0;
        while i < 4 {
            mask |= ((lanes[i] < 0) as u32) << i;
            i += 1;
        }
        mask
    }

    #[doc(alias = "i64x2.bitmask")]
    pub const fn i64x2_bitmask(self) -> u32 {
        let lanes = self.as_i64x2();
        let mut mask = 0u32;
        let mut i = 0;
        while i < 2 {
            mask |= ((lanes[i] < 0) as u32) << i;
            i += 1;
        }
        mask
    }

    #[doc(alias = "i8x16.popcnt")]
    pub const fn i8x16_popcnt(self) -> Self {
        let lanes = self.as_u8x16();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = lanes[i].count_ones() as u8;
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i8x16.shl")]
    pub const fn i8x16_shl(self, shift: u32) -> Self {
        let lanes = self.as_i8x16();
        let s = shift & 7;
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = lanes[i].wrapping_shl(s);
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.shl")]
    pub const fn i16x8_shl(self, shift: u32) -> Self {
        let lanes = self.as_i16x8();
        let s = shift & 15;
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = lanes[i].wrapping_shl(s);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.shl")]
    pub const fn i32x4_shl(self, shift: u32) -> Self {
        let lanes = self.as_i32x4();
        let s = shift & 31;
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = lanes[i].wrapping_shl(s);
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.shl")]
    pub const fn i64x2_shl(self, shift: u32) -> Self {
        let lanes = self.as_i64x2();
        let s = shift & 63;
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = lanes[i].wrapping_shl(s);
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.shr_s")]
    pub const fn i8x16_shr_s(self, shift: u32) -> Self {
        let lanes = self.as_i8x16();
        let s = shift & 7;
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.shr_s")]
    pub const fn i16x8_shr_s(self, shift: u32) -> Self {
        let lanes = self.as_i16x8();
        let s = shift & 15;
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.shr_s")]
    pub const fn i32x4_shr_s(self, shift: u32) -> Self {
        let lanes = self.as_i32x4();
        let s = shift & 31;
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.shr_s")]
    pub const fn i64x2_shr_s(self, shift: u32) -> Self {
        let lanes = self.as_i64x2();
        let s = shift & 63;
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.shr_u")]
    pub const fn i8x16_shr_u(self, shift: u32) -> Self {
        let lanes = self.as_u8x16();
        let s = shift & 7;
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.shr_u")]
    pub const fn i16x8_shr_u(self, shift: u32) -> Self {
        let lanes = self.as_u16x8();
        let s = shift & 15;
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i32x4.shr_u")]
    pub const fn i32x4_shr_u(self, shift: u32) -> Self {
        let lanes = self.as_u32x4();
        let s = shift & 31;
        let mut out = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_u32x4(out)
    }

    #[doc(alias = "i64x2.shr_u")]
    pub const fn i64x2_shr_u(self, shift: u32) -> Self {
        let lanes = self.as_u64x2();
        let s = shift & 63;
        let mut out = [0u64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = lanes[i] >> s;
            i += 1;
        }
        Self::from_u64x2(out)
    }

    #[doc(alias = "i8x16.add")]
    pub const fn i8x16_add(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].wrapping_add(b[i]);
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.add")]
    pub const fn i16x8_add(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].wrapping_add(b[i]);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.add")]
    pub const fn i32x4_add(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = a[i].wrapping_add(b[i]);
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.add")]
    pub const fn i64x2_add(self, rhs: Self) -> Self {
        let a = self.as_i64x2();
        let b = rhs.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = a[i].wrapping_add(b[i]);
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.sub")]
    pub const fn i8x16_sub(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].wrapping_sub(b[i]);
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.sub")]
    pub const fn i16x8_sub(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].wrapping_sub(b[i]);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.sub")]
    pub const fn i32x4_sub(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = a[i].wrapping_sub(b[i]);
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.sub")]
    pub const fn i64x2_sub(self, rhs: Self) -> Self {
        let a = self.as_i64x2();
        let b = rhs.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = a[i].wrapping_sub(b[i]);
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i16x8.mul")]
    pub const fn i16x8_mul(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].wrapping_mul(b[i]);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.mul")]
    pub const fn i32x4_mul(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = a[i].wrapping_mul(b[i]);
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.mul")]
    pub const fn i64x2_mul(self, rhs: Self) -> Self {
        let a = self.as_i64x2();
        let b = rhs.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = a[i].wrapping_mul(b[i]);
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.add_sat_s")]
    pub const fn i8x16_add_sat_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].saturating_add(b[i]);
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.add_sat_s")]
    pub const fn i16x8_add_sat_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].saturating_add(b[i]);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i8x16.add_sat_u")]
    pub const fn i8x16_add_sat_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].saturating_add(b[i]);
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.add_sat_u")]
    pub const fn i16x8_add_sat_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].saturating_add(b[i]);
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i8x16.sub_sat_s")]
    pub const fn i8x16_sub_sat_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].saturating_sub(b[i]);
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.sub_sat_s")]
    pub const fn i16x8_sub_sat_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].saturating_sub(b[i]);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i8x16.sub_sat_u")]
    pub const fn i8x16_sub_sat_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].saturating_sub(b[i]);
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.sub_sat_u")]
    pub const fn i16x8_sub_sat_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].saturating_sub(b[i]);
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i8x16.avgr_u")]
    pub const fn i8x16_avgr_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = ((a[i] as u16 + b[i] as u16 + 1) >> 1) as u8;
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.avgr_u")]
    pub const fn i16x8_avgr_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = ((a[i] as u32 + b[i] as u32 + 1) >> 1) as u16;
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i8x16.narrow_i16x8_s")]
    pub const fn i8x16_narrow_i16x8_s(a: Self, b: Self) -> Self {
        let av = a.as_i16x8();
        let bv = b.as_i16x8();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 8 {
            out[i] = saturate_i16_to_i8(av[i]);
            out[i + 8] = saturate_i16_to_i8(bv[i]);
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i8x16.narrow_i16x8_u")]
    pub const fn i8x16_narrow_i16x8_u(a: Self, b: Self) -> Self {
        let av = a.as_i16x8();
        let bv = b.as_i16x8();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 8 {
            out[i] = saturate_i16_to_u8(av[i]);
            out[i + 8] = saturate_i16_to_u8(bv[i]);
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.narrow_i32x4_s")]
    pub const fn i16x8_narrow_i32x4_s(a: Self, b: Self) -> Self {
        let av = a.as_i32x4();
        let bv = b.as_i32x4();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 4 {
            out[i] = saturate_i32_to_i16(av[i]);
            out[i + 4] = saturate_i32_to_i16(bv[i]);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i16x8.narrow_i32x4_u")]
    pub const fn i16x8_narrow_i32x4_u(a: Self, b: Self) -> Self {
        let av = a.as_i32x4();
        let bv = b.as_i32x4();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 4 {
            out[i] = saturate_i32_to_u16(av[i]);
            out[i + 4] = saturate_i32_to_u16(bv[i]);
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i16x8.extadd_pairwise_i8x16_s")]
    pub const fn i16x8_extadd_pairwise_i8x16_s(self) -> Self {
        let lanes = self.as_i8x16();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            let j = i * 2;
            out[i] = lanes[j] as i16 + lanes[j + 1] as i16;
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i16x8.extadd_pairwise_i8x16_u")]
    pub const fn i16x8_extadd_pairwise_i8x16_u(self) -> Self {
        let lanes = self.as_u8x16();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            let j = i * 2;
            out[i] = lanes[j] as u16 + lanes[j + 1] as u16;
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i32x4.extadd_pairwise_i16x8_s")]
    pub const fn i32x4_extadd_pairwise_i16x8_s(self) -> Self {
        let lanes = self.as_i16x8();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            let j = i * 2;
            out[i] = lanes[j] as i32 + lanes[j + 1] as i32;
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i32x4.extadd_pairwise_i16x8_u")]
    pub const fn i32x4_extadd_pairwise_i16x8_u(self) -> Self {
        let lanes = self.as_u16x8();
        let mut out = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            let j = i * 2;
            out[i] = lanes[j] as u32 + lanes[j + 1] as u32;
            i += 1;
        }
        Self::from_u32x4(out)
    }

    #[doc(alias = "i16x8.extend_low_i8x16_s")]
    pub const fn i16x8_extend_low_i8x16_s(self) -> Self {
        let lanes = self.as_i8x16();
        Self::from_i16x8([
            lanes[0] as i16,
            lanes[1] as i16,
            lanes[2] as i16,
            lanes[3] as i16,
            lanes[4] as i16,
            lanes[5] as i16,
            lanes[6] as i16,
            lanes[7] as i16,
        ])
    }

    #[doc(alias = "i16x8.extend_low_i8x16_u")]
    pub const fn i16x8_extend_low_i8x16_u(self) -> Self {
        let lanes = self.as_u8x16();
        Self::from_u16x8([
            lanes[0] as u16,
            lanes[1] as u16,
            lanes[2] as u16,
            lanes[3] as u16,
            lanes[4] as u16,
            lanes[5] as u16,
            lanes[6] as u16,
            lanes[7] as u16,
        ])
    }

    #[doc(alias = "i16x8.extend_high_i8x16_s")]
    pub const fn i16x8_extend_high_i8x16_s(self) -> Self {
        let lanes = self.as_i8x16();
        Self::from_i16x8([
            lanes[8] as i16,
            lanes[9] as i16,
            lanes[10] as i16,
            lanes[11] as i16,
            lanes[12] as i16,
            lanes[13] as i16,
            lanes[14] as i16,
            lanes[15] as i16,
        ])
    }

    #[doc(alias = "i16x8.extend_high_i8x16_u")]
    pub const fn i16x8_extend_high_i8x16_u(self) -> Self {
        let lanes = self.as_u8x16();
        Self::from_u16x8([
            lanes[8] as u16,
            lanes[9] as u16,
            lanes[10] as u16,
            lanes[11] as u16,
            lanes[12] as u16,
            lanes[13] as u16,
            lanes[14] as u16,
            lanes[15] as u16,
        ])
    }

    #[doc(alias = "i32x4.extend_low_i16x8_s")]
    pub const fn i32x4_extend_low_i16x8_s(self) -> Self {
        let lanes = self.as_i16x8();
        Self::from_i32x4([lanes[0] as i32, lanes[1] as i32, lanes[2] as i32, lanes[3] as i32])
    }

    #[doc(alias = "i32x4.extend_low_i16x8_u")]
    pub const fn i32x4_extend_low_i16x8_u(self) -> Self {
        let lanes = self.as_u16x8();
        Self::from_u32x4([lanes[0] as u32, lanes[1] as u32, lanes[2] as u32, lanes[3] as u32])
    }

    #[doc(alias = "i32x4.extend_high_i16x8_s")]
    pub const fn i32x4_extend_high_i16x8_s(self) -> Self {
        let lanes = self.as_i16x8();
        Self::from_i32x4([lanes[4] as i32, lanes[5] as i32, lanes[6] as i32, lanes[7] as i32])
    }

    #[doc(alias = "i32x4.extend_high_i16x8_u")]
    pub const fn i32x4_extend_high_i16x8_u(self) -> Self {
        let lanes = self.as_u16x8();
        Self::from_u32x4([lanes[4] as u32, lanes[5] as u32, lanes[6] as u32, lanes[7] as u32])
    }

    #[doc(alias = "i64x2.extend_low_i32x4_s")]
    pub const fn i64x2_extend_low_i32x4_s(self) -> Self {
        let lanes = self.as_i32x4();
        Self::from_i64x2([lanes[0] as i64, lanes[1] as i64])
    }

    #[doc(alias = "i64x2.extend_low_i32x4_u")]
    pub const fn i64x2_extend_low_i32x4_u(self) -> Self {
        let lanes = self.as_u32x4();
        Self::from_u64x2([lanes[0] as u64, lanes[1] as u64])
    }

    #[doc(alias = "i64x2.extend_high_i32x4_s")]
    pub const fn i64x2_extend_high_i32x4_s(self) -> Self {
        let lanes = self.as_i32x4();
        Self::from_i64x2([lanes[2] as i64, lanes[3] as i64])
    }

    #[doc(alias = "i64x2.extend_high_i32x4_u")]
    pub const fn i64x2_extend_high_i32x4_u(self) -> Self {
        let lanes = self.as_u32x4();
        Self::from_u64x2([lanes[2] as u64, lanes[3] as u64])
    }

    #[doc(alias = "i16x8.extmul_low_i8x16_s")]
    pub const fn i16x8_extmul_low_i8x16_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = (a[i] as i16).wrapping_mul(b[i] as i16);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i16x8.extmul_low_i8x16_u")]
    pub const fn i16x8_extmul_low_i8x16_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = (a[i] as u16) * (b[i] as u16);
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i16x8.extmul_high_i8x16_s")]
    pub const fn i16x8_extmul_high_i8x16_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = (a[i + 8] as i16).wrapping_mul(b[i + 8] as i16);
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i16x8.extmul_high_i8x16_u")]
    pub const fn i16x8_extmul_high_i8x16_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = (a[i + 8] as u16) * (b[i + 8] as u16);
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i32x4.extmul_low_i16x8_s")]
    pub const fn i32x4_extmul_low_i16x8_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = (a[i] as i32).wrapping_mul(b[i] as i32);
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i32x4.extmul_low_i16x8_u")]
    pub const fn i32x4_extmul_low_i16x8_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = (a[i] as u32) * (b[i] as u32);
            i += 1;
        }
        Self::from_u32x4(out)
    }

    #[doc(alias = "i32x4.extmul_high_i16x8_s")]
    pub const fn i32x4_extmul_high_i16x8_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = (a[i + 4] as i32).wrapping_mul(b[i + 4] as i32);
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i32x4.extmul_high_i16x8_u")]
    pub const fn i32x4_extmul_high_i16x8_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = (a[i + 4] as u32) * (b[i + 4] as u32);
            i += 1;
        }
        Self::from_u32x4(out)
    }

    #[doc(alias = "i64x2.extmul_low_i32x4_s")]
    pub const fn i64x2_extmul_low_i32x4_s(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = (a[i] as i64).wrapping_mul(b[i] as i64);
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i64x2.extmul_low_i32x4_u")]
    pub const fn i64x2_extmul_low_i32x4_u(self, rhs: Self) -> Self {
        let a = self.as_u32x4();
        let b = rhs.as_u32x4();
        let mut out = [0u64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = (a[i] as u64) * (b[i] as u64);
            i += 1;
        }
        Self::from_u64x2(out)
    }

    #[doc(alias = "i64x2.extmul_high_i32x4_s")]
    pub const fn i64x2_extmul_high_i32x4_s(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = (a[i + 2] as i64).wrapping_mul(b[i + 2] as i64);
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i64x2.extmul_high_i32x4_u")]
    pub const fn i64x2_extmul_high_i32x4_u(self, rhs: Self) -> Self {
        let a = self.as_u32x4();
        let b = rhs.as_u32x4();
        let mut out = [0u64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = (a[i + 2] as u64) * (b[i + 2] as u64);
            i += 1;
        }
        Self::from_u64x2(out)
    }

    #[doc(alias = "i16x8.q15mulr_sat_s")]
    pub const fn i16x8_q15mulr_sat_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            let r = ((a[i] as i32 * b[i] as i32) + (1 << 14)) >> 15; // 2^14: Q15 rounding
            out[i] = if r > i16::MAX as i32 {
                i16::MAX
            } else if r < i16::MIN as i32 {
                i16::MIN
            } else {
                r as i16
            };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.dot_i16x8_s")]
    pub const fn i32x4_dot_i16x8_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        Self::from_i32x4([
            (a[0] as i32).wrapping_mul(b[0] as i32).wrapping_add((a[1] as i32).wrapping_mul(b[1] as i32)),
            (a[2] as i32).wrapping_mul(b[2] as i32).wrapping_add((a[3] as i32).wrapping_mul(b[3] as i32)),
            (a[4] as i32).wrapping_mul(b[4] as i32).wrapping_add((a[5] as i32).wrapping_mul(b[5] as i32)),
            (a[6] as i32).wrapping_mul(b[6] as i32).wrapping_add((a[7] as i32).wrapping_mul(b[7] as i32)),
        ])
    }

    #[doc(alias = "i8x16.eq")]
    pub const fn i8x16_eq(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] == b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.eq")]
    pub const fn i16x8_eq(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] == b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.eq")]
    pub const fn i32x4_eq(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] == b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.eq")]
    pub const fn i64x2_eq(self, rhs: Self) -> Self {
        let a = self.as_i64x2();
        let b = rhs.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] == b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.ne")]
    pub const fn i8x16_ne(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] != b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.ne")]
    pub const fn i16x8_ne(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] != b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.ne")]
    pub const fn i32x4_ne(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] != b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.ne")]
    pub const fn i64x2_ne(self, rhs: Self) -> Self {
        let a = self.as_i64x2();
        let b = rhs.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] != b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.lt_s")]
    pub const fn i8x16_lt_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.lt_s")]
    pub const fn i16x8_lt_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.lt_s")]
    pub const fn i32x4_lt_s(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.lt_s")]
    pub const fn i64x2_lt_s(self, rhs: Self) -> Self {
        let a = self.as_i64x2();
        let b = rhs.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.lt_u")]
    pub const fn i8x16_lt_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.lt_u")]
    pub const fn i16x8_lt_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.lt_u")]
    pub const fn i32x4_lt_u(self, rhs: Self) -> Self {
        let a = self.as_u32x4();
        let b = rhs.as_u32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i8x16.gt_s")]
    pub const fn i8x16_gt_s(self, rhs: Self) -> Self {
        rhs.i8x16_lt_s(self)
    }

    #[doc(alias = "i16x8.gt_s")]
    pub const fn i16x8_gt_s(self, rhs: Self) -> Self {
        rhs.i16x8_lt_s(self)
    }

    #[doc(alias = "i32x4.gt_s")]
    pub const fn i32x4_gt_s(self, rhs: Self) -> Self {
        rhs.i32x4_lt_s(self)
    }

    #[doc(alias = "i64x2.gt_s")]
    pub const fn i64x2_gt_s(self, rhs: Self) -> Self {
        rhs.i64x2_lt_s(self)
    }

    #[doc(alias = "i8x16.gt_u")]
    pub const fn i8x16_gt_u(self, rhs: Self) -> Self {
        rhs.i8x16_lt_u(self)
    }

    #[doc(alias = "i16x8.gt_u")]
    pub const fn i16x8_gt_u(self, rhs: Self) -> Self {
        rhs.i16x8_lt_u(self)
    }

    #[doc(alias = "i32x4.gt_u")]
    pub const fn i32x4_gt_u(self, rhs: Self) -> Self {
        rhs.i32x4_lt_u(self)
    }

    #[doc(alias = "i8x16.le_s")]
    pub const fn i8x16_le_s(self, rhs: Self) -> Self {
        rhs.i8x16_ge_s(self)
    }

    #[doc(alias = "i16x8.le_s")]
    pub const fn i16x8_le_s(self, rhs: Self) -> Self {
        rhs.i16x8_ge_s(self)
    }

    #[doc(alias = "i32x4.le_s")]
    pub const fn i32x4_le_s(self, rhs: Self) -> Self {
        rhs.i32x4_ge_s(self)
    }

    #[doc(alias = "i64x2.le_s")]
    pub const fn i64x2_le_s(self, rhs: Self) -> Self {
        rhs.i64x2_ge_s(self)
    }

    #[doc(alias = "i8x16.le_u")]
    pub const fn i8x16_le_u(self, rhs: Self) -> Self {
        rhs.i8x16_ge_u(self)
    }

    #[doc(alias = "i16x8.le_u")]
    pub const fn i16x8_le_u(self, rhs: Self) -> Self {
        rhs.i16x8_ge_u(self)
    }

    #[doc(alias = "i32x4.le_u")]
    pub const fn i32x4_le_u(self, rhs: Self) -> Self {
        rhs.i32x4_ge_u(self)
    }

    #[doc(alias = "i8x16.ge_s")]
    pub const fn i8x16_ge_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.ge_s")]
    pub const fn i16x8_ge_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.ge_s")]
    pub const fn i32x4_ge_s(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.ge_s")]
    pub const fn i64x2_ge_s(self, rhs: Self) -> Self {
        let a = self.as_i64x2();
        let b = rhs.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.ge_u")]
    pub const fn i8x16_ge_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.ge_u")]
    pub const fn i16x8_ge_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.ge_u")]
    pub const fn i32x4_ge_u(self, rhs: Self) -> Self {
        let a = self.as_u32x4();
        let b = rhs.as_u32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i8x16.abs")]
    pub const fn i8x16_abs(self) -> Self {
        let a = self.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].wrapping_abs();
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.abs")]
    pub const fn i16x8_abs(self) -> Self {
        let a = self.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].wrapping_abs();
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.abs")]
    pub const fn i32x4_abs(self) -> Self {
        let a = self.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = a[i].wrapping_abs();
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.abs")]
    pub const fn i64x2_abs(self) -> Self {
        let a = self.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = a[i].wrapping_abs();
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.neg")]
    pub const fn i8x16_neg(self) -> Self {
        let a = self.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = a[i].wrapping_neg();
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.neg")]
    pub const fn i16x8_neg(self) -> Self {
        let a = self.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = a[i].wrapping_neg();
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.neg")]
    pub const fn i32x4_neg(self) -> Self {
        let a = self.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = a[i].wrapping_neg();
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i64x2.neg")]
    pub const fn i64x2_neg(self) -> Self {
        let a = self.as_i64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = a[i].wrapping_neg();
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "i8x16.min_s")]
    pub const fn i8x16_min_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] < b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.min_s")]
    pub const fn i16x8_min_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] < b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.min_s")]
    pub const fn i32x4_min_s(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] < b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i8x16.min_u")]
    pub const fn i8x16_min_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] < b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.min_u")]
    pub const fn i16x8_min_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] < b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i32x4.min_u")]
    pub const fn i32x4_min_u(self, rhs: Self) -> Self {
        let a = self.as_u32x4();
        let b = rhs.as_u32x4();
        let mut out = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] < b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_u32x4(out)
    }

    #[doc(alias = "i8x16.max_s")]
    pub const fn i8x16_max_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] > b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_i8x16(out)
    }

    #[doc(alias = "i16x8.max_s")]
    pub const fn i16x8_max_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] > b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.max_s")]
    pub const fn i32x4_max_s(self, rhs: Self) -> Self {
        let a = self.as_i32x4();
        let b = rhs.as_i32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] > b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "i8x16.max_u")]
    pub const fn i8x16_max_u(self, rhs: Self) -> Self {
        let a = self.as_u8x16();
        let b = rhs.as_u8x16();
        let mut out = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            out[i] = if a[i] > b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_u8x16(out)
    }

    #[doc(alias = "i16x8.max_u")]
    pub const fn i16x8_max_u(self, rhs: Self) -> Self {
        let a = self.as_u16x8();
        let b = rhs.as_u16x8();
        let mut out = [0u16; 8];
        let mut i = 0;
        while i < 8 {
            out[i] = if a[i] > b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_u16x8(out)
    }

    #[doc(alias = "i32x4.max_u")]
    pub const fn i32x4_max_u(self, rhs: Self) -> Self {
        let a = self.as_u32x4();
        let b = rhs.as_u32x4();
        let mut out = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] > b[i] { a[i] } else { b[i] };
            i += 1;
        }
        Self::from_u32x4(out)
    }

    #[doc(alias = "f32x4.eq")]
    pub const fn f32x4_eq(self, rhs: Self) -> Self {
        let a = self.as_f32x4();
        let b = rhs.as_f32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] == b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "f64x2.eq")]
    pub const fn f64x2_eq(self, rhs: Self) -> Self {
        let a = self.as_f64x2();
        let b = rhs.as_f64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] == b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "f32x4.ne")]
    pub const fn f32x4_ne(self, rhs: Self) -> Self {
        let a = self.as_f32x4();
        let b = rhs.as_f32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] != b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "f64x2.ne")]
    pub const fn f64x2_ne(self, rhs: Self) -> Self {
        let a = self.as_f64x2();
        let b = rhs.as_f64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] != b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "f32x4.lt")]
    pub const fn f32x4_lt(self, rhs: Self) -> Self {
        let a = self.as_f32x4();
        let b = rhs.as_f32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "f64x2.lt")]
    pub const fn f64x2_lt(self, rhs: Self) -> Self {
        let a = self.as_f64x2();
        let b = rhs.as_f64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] < b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "f32x4.gt")]
    pub const fn f32x4_gt(self, rhs: Self) -> Self {
        rhs.f32x4_lt(self)
    }

    #[doc(alias = "f64x2.gt")]
    pub const fn f64x2_gt(self, rhs: Self) -> Self {
        rhs.f64x2_lt(self)
    }

    #[doc(alias = "f32x4.le")]
    pub const fn f32x4_le(self, rhs: Self) -> Self {
        let a = self.as_f32x4();
        let b = rhs.as_f32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] <= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "f64x2.le")]
    pub const fn f64x2_le(self, rhs: Self) -> Self {
        let a = self.as_f64x2();
        let b = rhs.as_f64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] <= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "f32x4.ge")]
    pub const fn f32x4_ge(self, rhs: Self) -> Self {
        let a = self.as_f32x4();
        let b = rhs.as_f32x4();
        let mut out = [0i32; 4];
        let mut i = 0;
        while i < 4 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i32x4(out)
    }

    #[doc(alias = "f64x2.ge")]
    pub const fn f64x2_ge(self, rhs: Self) -> Self {
        let a = self.as_f64x2();
        let b = rhs.as_f64x2();
        let mut out = [0i64; 2];
        let mut i = 0;
        while i < 2 {
            out[i] = if a[i] >= b[i] { -1 } else { 0 };
            i += 1;
        }
        Self::from_i64x2(out)
    }

    #[doc(alias = "f32x4.ceil")]
    pub fn f32x4_ceil(self) -> Self {
        self.map_f32x4(|x| canonicalize_simd_f32_nan(x.ceil()))
    }

    #[doc(alias = "f64x2.ceil")]
    pub fn f64x2_ceil(self) -> Self {
        self.map_f64x2(|x| canonicalize_simd_f64_nan(x.ceil()))
    }

    #[doc(alias = "f32x4.floor")]
    pub fn f32x4_floor(self) -> Self {
        self.map_f32x4(|x| canonicalize_simd_f32_nan(x.floor()))
    }

    #[doc(alias = "f64x2.floor")]
    pub fn f64x2_floor(self) -> Self {
        self.map_f64x2(|x| canonicalize_simd_f64_nan(x.floor()))
    }

    #[doc(alias = "f32x4.trunc")]
    pub fn f32x4_trunc(self) -> Self {
        self.map_f32x4(|x| canonicalize_simd_f32_nan(x.trunc()))
    }

    #[doc(alias = "f64x2.trunc")]
    pub fn f64x2_trunc(self) -> Self {
        self.map_f64x2(|x| canonicalize_simd_f64_nan(x.trunc()))
    }

    #[doc(alias = "f32x4.nearest")]
    pub fn f32x4_nearest(self) -> Self {
        self.map_f32x4(|x| canonicalize_simd_f32_nan(TinywasmFloatExt::tw_nearest(x)))
    }

    #[doc(alias = "f64x2.nearest")]
    pub fn f64x2_nearest(self) -> Self {
        self.map_f64x2(|x| canonicalize_simd_f64_nan(TinywasmFloatExt::tw_nearest(x)))
    }

    #[doc(alias = "f32x4.abs")]
    pub fn f32x4_abs(self) -> Self {
        self.map_f32x4(f32::abs)
    }

    #[doc(alias = "f64x2.abs")]
    pub fn f64x2_abs(self) -> Self {
        self.map_f64x2(f64::abs)
    }

    #[doc(alias = "f32x4.neg")]
    pub fn f32x4_neg(self) -> Self {
        self.map_f32x4(|x| -x)
    }

    #[doc(alias = "f64x2.neg")]
    pub fn f64x2_neg(self) -> Self {
        self.map_f64x2(|x| -x)
    }

    #[doc(alias = "f32x4.sqrt")]
    pub fn f32x4_sqrt(self) -> Self {
        self.map_f32x4(|x| canonicalize_simd_f32_nan(x.sqrt()))
    }

    #[doc(alias = "f64x2.sqrt")]
    pub fn f64x2_sqrt(self) -> Self {
        self.map_f64x2(|x| canonicalize_simd_f64_nan(x.sqrt()))
    }

    #[doc(alias = "f32x4.add")]
    pub fn f32x4_add(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a + b))
    }

    #[doc(alias = "f64x2.add")]
    pub fn f64x2_add(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a + b))
    }

    #[doc(alias = "f32x4.sub")]
    pub fn f32x4_sub(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a - b))
    }

    #[doc(alias = "f64x2.sub")]
    pub fn f64x2_sub(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a - b))
    }

    #[doc(alias = "f32x4.mul")]
    pub fn f32x4_mul(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a * b))
    }

    #[doc(alias = "f64x2.mul")]
    pub fn f64x2_mul(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a * b))
    }

    #[doc(alias = "f32x4.div")]
    pub fn f32x4_div(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a / b))
    }

    #[doc(alias = "f64x2.div")]
    pub fn f64x2_div(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a / b))
    }

    #[doc(alias = "f32x4.min")]
    pub fn f32x4_min(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, TinywasmFloatExt::tw_minimum)
    }

    #[doc(alias = "f64x2.min")]
    pub fn f64x2_min(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, TinywasmFloatExt::tw_minimum)
    }

    #[doc(alias = "f32x4.max")]
    pub fn f32x4_max(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, TinywasmFloatExt::tw_maximum)
    }

    #[doc(alias = "f64x2.max")]
    pub fn f64x2_max(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, TinywasmFloatExt::tw_maximum)
    }

    #[doc(alias = "f32x4.pmin")]
    pub fn f32x4_pmin(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, |a, b| if b < a { b } else { a })
    }

    #[doc(alias = "f64x2.pmin")]
    pub fn f64x2_pmin(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, |a, b| if b < a { b } else { a })
    }

    #[doc(alias = "f32x4.pmax")]
    pub fn f32x4_pmax(self, rhs: Self) -> Self {
        self.zip_f32x4(rhs, |a, b| if b > a { b } else { a })
    }

    #[doc(alias = "f64x2.pmax")]
    pub fn f64x2_pmax(self, rhs: Self) -> Self {
        self.zip_f64x2(rhs, |a, b| if b > a { b } else { a })
    }

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

    pub const fn splat_i16(src: i16) -> Self {
        let mut result_bytes = [0u8; 16];
        let bytes = src.to_le_bytes();
        let mut i = 0;
        while i < 8 {
            result_bytes[i * 2] = bytes[0];
            result_bytes[i * 2 + 1] = bytes[1];
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_i32(src: i32) -> Self {
        let mut result_bytes = [0u8; 16];
        let bytes = src.to_le_bytes();
        let mut i = 0;
        while i < 4 {
            result_bytes[i * 4] = bytes[0];
            result_bytes[i * 4 + 1] = bytes[1];
            result_bytes[i * 4 + 2] = bytes[2];
            result_bytes[i * 4 + 3] = bytes[3];
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_i64(src: i64) -> Self {
        let mut result_bytes = [0u8; 16];
        let bytes = src.to_le_bytes();
        let mut i = 0;
        while i < 2 {
            result_bytes[i * 8] = bytes[0];
            result_bytes[i * 8 + 1] = bytes[1];
            result_bytes[i * 8 + 2] = bytes[2];
            result_bytes[i * 8 + 3] = bytes[3];
            result_bytes[i * 8 + 4] = bytes[4];
            result_bytes[i * 8 + 5] = bytes[5];
            result_bytes[i * 8 + 6] = bytes[6];
            result_bytes[i * 8 + 7] = bytes[7];
            i += 1;
        }
        Self::from_le_bytes(result_bytes)
    }

    pub const fn splat_f32(src: f32) -> Self {
        Self::splat_i32(src.to_bits() as i32)
    }

    pub const fn splat_f64(src: f64) -> Self {
        Self::splat_i64(src.to_bits() as i64)
    }

    pub const fn extract_lane_i8(self, lane: u8) -> i8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane] as i8
    }

    pub const fn extract_lane_u8(self, lane: u8) -> u8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane]
    }

    pub const fn extract_lane_i16(self, lane: u8) -> i16 {
        debug_assert!(lane < 8);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 2;
        i16::from_le_bytes([bytes[start], bytes[start + 1]])
    }

    pub const fn extract_lane_u16(self, lane: u8) -> u16 {
        debug_assert!(lane < 8);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 2;
        u16::from_le_bytes([bytes[start], bytes[start + 1]])
    }

    pub const fn extract_lane_i32(self, lane: u8) -> i32 {
        debug_assert!(lane < 4);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 4;
        i32::from_le_bytes([bytes[start], bytes[start + 1], bytes[start + 2], bytes[start + 3]])
    }

    pub const fn extract_lane_i64(self, lane: u8) -> i64 {
        debug_assert!(lane < 2);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        let start = lane * 8;
        i64::from_le_bytes([
            bytes[start],
            bytes[start + 1],
            bytes[start + 2],
            bytes[start + 3],
            bytes[start + 4],
            bytes[start + 5],
            bytes[start + 6],
            bytes[start + 7],
        ])
    }

    pub const fn extract_lane_f32(self, lane: u8) -> f32 {
        f32::from_bits(self.extract_lane_i32(lane) as u32)
    }

    pub const fn extract_lane_f64(self, lane: u8) -> f64 {
        f64::from_bits(self.extract_lane_i64(lane) as u64)
    }

    const fn replace_lane_bytes<const LANE_BYTES: usize>(
        self,
        lane: u8,
        value: [u8; LANE_BYTES],
        lane_count: u8,
    ) -> Self {
        debug_assert!(lane < lane_count);
        let mut bytes = self.to_le_bytes();
        let mut i = 0;
        let start = lane as usize * LANE_BYTES;
        while i < LANE_BYTES {
            bytes[start + i] = value[i];
            i += 1;
        }
        Self::from_le_bytes(bytes)
    }
}

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

impl core::ops::Not for Value128 {
    type Output = Self;
    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

impl core::ops::BitAnd for Value128 {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl core::ops::BitOr for Value128 {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitXor for Value128 {
    type Output = Self;
    fn bitxor(self, rhs: Self) -> Self::Output {
        Self(self.0 ^ rhs.0)
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
    if x > i8::MAX as i16 {
        i8::MAX
    } else if x < i8::MIN as i16 {
        i8::MIN
    } else {
        x as i8
    }
}

const fn saturate_i16_to_u8(x: i16) -> u8 {
    if x <= 0 {
        0
    } else if x > u8::MAX as i16 {
        u8::MAX
    } else {
        x as u8
    }
}

const fn saturate_i32_to_i16(x: i32) -> i16 {
    if x > i16::MAX as i32 {
        i16::MAX
    } else if x < i16::MIN as i32 {
        i16::MIN
    } else {
        x as i16
    }
}

const fn saturate_i32_to_u16(x: i32) -> u16 {
    if x <= 0 {
        0
    } else if x > u16::MAX as i32 {
        u16::MAX
    } else {
        x as u16
    }
}

fn trunc_sat_f32_to_i32(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else if v <= i32::MIN as f32 - (1 << 8) as f32 {
        i32::MIN
    } else if v >= (i32::MAX as f32 + 1.0) {
        i32::MAX
    } else {
        v.trunc() as i32
    }
}

fn trunc_sat_f32_to_u32(v: f32) -> u32 {
    if v.is_nan() || v <= -1.0_f32 {
        0
    } else if v >= (u32::MAX as f32 + 1.0) {
        u32::MAX
    } else {
        v.trunc() as u32
    }
}

fn trunc_sat_f64_to_i32(v: f64) -> i32 {
    if v.is_nan() {
        0
    } else if v <= i32::MIN as f64 - 1.0_f64 {
        i32::MIN
    } else if v >= (i32::MAX as f64 + 1.0) {
        i32::MAX
    } else {
        v.trunc() as i32
    }
}

fn trunc_sat_f64_to_u32(v: f64) -> u32 {
    if v.is_nan() || v <= -1.0_f64 {
        0
    } else if v >= (u32::MAX as f64 + 1.0) {
        u32::MAX
    } else {
        v.trunc() as u32
    }
}
