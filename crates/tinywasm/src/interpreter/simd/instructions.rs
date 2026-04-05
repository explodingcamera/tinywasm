use super::super::num_helpers::TinywasmFloatExt;
use super::Value128;
use super::utils::*;

#[cfg(not(feature = "std"))]
use super::super::no_std_floats::NoStdFloatExt;
#[cfg(target_arch = "wasm32")]
use core::arch::wasm32 as wasm;
#[cfg(target_arch = "wasm64")]
use core::arch::wasm64 as wasm;
#[cfg(all(
    feature = "simd-x86",
    target_arch = "x86_64",
    target_feature = "sse4.2",
    target_feature = "avx",
    target_feature = "avx2",
    target_feature = "bmi1",
    target_feature = "bmi2",
    target_feature = "fma",
    target_feature = "lzcnt",
    target_feature = "movbe",
    target_feature = "popcnt"
))]
use core::arch::x86_64 as x86;

impl Value128 {
    #[doc(alias = "v128.any_true")]
    pub fn v128_any_true(self) -> bool {
        simd_impl! {
            wasm => { wasm::v128_any_true(self.to_wasm_v128()) }
            generic => { self.0 != 0 }
        }
    }

    #[doc(alias = "v128.not")]
    pub fn v128_not(self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_not(self.to_wasm_v128())) }
            generic => { Self(!self.0) }
        }
    }

    #[doc(alias = "v128.and")]
    pub fn v128_and(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_and(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { Self(self.0 & rhs.0) }
        }
    }

    #[doc(alias = "v128.andnot")]
    pub fn v128_andnot(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_andnot(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { Self(self.0 & !rhs.0) }
        }
    }

    #[doc(alias = "v128.or")]
    pub fn v128_or(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_or(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { Self(self.0 | rhs.0) }
        }
    }

    #[doc(alias = "v128.xor")]
    pub fn v128_xor(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_xor(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { Self(self.0 ^ rhs.0) }
        }
    }

    #[doc(alias = "v128.bitselect")]
    pub fn v128_bitselect(v1: Self, v2: Self, c: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_bitselect(v1.to_wasm_v128(), v2.to_wasm_v128(), c.to_wasm_v128())) }
            generic => { Self((v1.0 & c.0) | (v2.0 & !c.0)) }
        }
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
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::i8x16_swizzle(self.to_wasm_v128(), s.to_wasm_v128())) }
            x86 => {
                let a = self.to_le_bytes();
                let idx = s.to_le_bytes();
                let mut mask = [0u8; 16];
                for i in 0..16 {
                    let j = idx[i];
                    mask[i] = if j < 16 { j & 0x0f } else { 0x80 };
                }

                // SAFETY: `a`, `mask`, and `out` are valid 16-byte buffers, and `_mm_loadu/_mm_storeu` support unaligned accesses.
                #[allow(unsafe_code)]
                let out = unsafe {
                    let a_vec = x86::_mm_loadu_si128(a.as_ptr().cast::<x86::__m128i>());
                    let mask_vec = x86::_mm_loadu_si128(mask.as_ptr().cast::<x86::__m128i>());
                    let result = x86::_mm_shuffle_epi8(a_vec, mask_vec);
                    let mut out = [0u8; 16];
                    x86::_mm_storeu_si128(out.as_mut_ptr().cast::<x86::__m128i>(), result);
                    out
                };
                Self::from_le_bytes(out)
            }
            generic => {
                let a = self.to_le_bytes();
                let idx = s.to_le_bytes();
                let mut out = [0u8; 16];
                for i in 0..16 {
                    let j = idx[i];
                    let lane = a[(j & 0x0f) as usize];
                    out[i] = if j < 16 { lane } else { 0 };
                }
                Self::from_le_bytes(out)
            }
        }
    }

    #[doc(alias = "i8x16.relaxed_swizzle")]
    pub fn i8x16_relaxed_swizzle(self, s: Self) -> Self {
        self.i8x16_swizzle(s)
    }

    #[doc(alias = "i8x16.shuffle")]
    pub fn i8x16_shuffle(a: Self, b: Self, idx: [u8; 16]) -> Self {
        simd_impl! {
            x86 => {
                let a_bytes = a.to_le_bytes();
                let b_bytes = b.to_le_bytes();
                let mut mask_a = [0u8; 16];
                let mut mask_b = [0u8; 16];
                for i in 0..16 {
                    let j = idx[i] & 31;
                    mask_a[i] = if j < 16 { j } else { 0x80 };
                    mask_b[i] = if j < 16 { 0x80 } else { j & 0x0f };
                }

                // SAFETY: all inputs are valid 16-byte buffers, and `_mm_loadu/_mm_storeu` support unaligned accesses.
                #[allow(unsafe_code)]
                let out = unsafe {
                    let a_vec = x86::_mm_loadu_si128(a_bytes.as_ptr().cast::<x86::__m128i>());
                    let b_vec = x86::_mm_loadu_si128(b_bytes.as_ptr().cast::<x86::__m128i>());
                    let mask_a_vec = x86::_mm_loadu_si128(mask_a.as_ptr().cast::<x86::__m128i>());
                    let mask_b_vec = x86::_mm_loadu_si128(mask_b.as_ptr().cast::<x86::__m128i>());
                    let a_part = x86::_mm_shuffle_epi8(a_vec, mask_a_vec);
                    let b_part = x86::_mm_shuffle_epi8(b_vec, mask_b_vec);
                    let result = x86::_mm_or_si128(a_part, b_part);
                    let mut out = [0u8; 16];
                    x86::_mm_storeu_si128(out.as_mut_ptr().cast::<x86::__m128i>(), result);
                    out
                };
                Self::from_le_bytes(out)
            }
            generic => {
                let a_bytes = a.to_le_bytes();
                let b_bytes = b.to_le_bytes();
                let mut out = [0u8; 16];
                for i in 0..16 {
                    let j = idx[i] & 31;
                    out[i] = if j < 16 { a_bytes[j as usize] } else { b_bytes[(j & 0x0f) as usize] };
                }
                Self::from_le_bytes(out)
            }
        }
    }

    #[doc(alias = "i8x16.splat")]
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

    #[doc(alias = "i8x16.shl")]
    pub fn i8x16_shl(self, shift: u32) -> Self {
        simd_shift_left!(self, shift, i8, 16, as_i8x16, from_i8x16, 7)
    }
    #[doc(alias = "i16x8.shl")]
    pub fn i16x8_shl(self, shift: u32) -> Self {
        simd_shift_left!(self, shift, i16, 8, as_i16x8, from_i16x8, 15)
    }
    #[doc(alias = "i32x4.shl")]
    pub fn i32x4_shl(self, shift: u32) -> Self {
        simd_shift_left!(self, shift, i32, 4, as_i32x4, from_i32x4, 31)
    }
    #[doc(alias = "i64x2.shl")]
    pub fn i64x2_shl(self, shift: u32) -> Self {
        simd_shift_left!(self, shift, i64, 2, as_i64x2, from_i64x2, 63)
    }

    #[doc(alias = "i8x16.shr_s")]
    pub fn i8x16_shr_s(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, i8, 16, as_i8x16, from_i8x16, 7)
    }
    #[doc(alias = "i16x8.shr_s")]
    pub fn i16x8_shr_s(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, i16, 8, as_i16x8, from_i16x8, 15)
    }
    #[doc(alias = "i32x4.shr_s")]
    pub fn i32x4_shr_s(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, i32, 4, as_i32x4, from_i32x4, 31)
    }
    #[doc(alias = "i64x2.shr_s")]
    pub fn i64x2_shr_s(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, i64, 2, as_i64x2, from_i64x2, 63)
    }

    #[doc(alias = "i8x16.shr_u")]
    pub fn i8x16_shr_u(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, u8, 16, as_u8x16, from_u8x16, 7)
    }
    #[doc(alias = "i16x8.shr_u")]
    pub fn i16x8_shr_u(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, u16, 8, as_u16x8, from_u16x8, 15)
    }
    #[doc(alias = "i32x4.shr_u")]
    pub fn i32x4_shr_u(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, u32, 4, as_u32x4, from_u32x4, 31)
    }
    #[doc(alias = "i64x2.shr_u")]
    pub fn i64x2_shr_u(self, shift: u32) -> Self {
        simd_shift_right!(self, shift, u64, 2, as_u64x2, from_u64x2, 63)
    }

    #[doc(alias = "i8x16.add")]
    pub fn i8x16_add(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i8x16_add, i8, 16, as_i8x16, from_i8x16, wrapping_add)
    }
    #[doc(alias = "i16x8.add")]
    pub fn i16x8_add(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i16x8_add, i16, 8, as_i16x8, from_i16x8, wrapping_add)
    }
    #[doc(alias = "i32x4.add")]
    pub fn i32x4_add(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i32x4_add, i32, 4, as_i32x4, from_i32x4, wrapping_add)
    }
    #[doc(alias = "i64x2.add")]
    pub fn i64x2_add(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i64x2_add, i64, 2, as_i64x2, from_i64x2, wrapping_add)
    }
    #[doc(alias = "i8x16.sub")]
    pub fn i8x16_sub(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i8x16_sub, i8, 16, as_i8x16, from_i8x16, wrapping_sub)
    }
    #[doc(alias = "i16x8.sub")]
    pub fn i16x8_sub(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i16x8_sub, i16, 8, as_i16x8, from_i16x8, wrapping_sub)
    }
    #[doc(alias = "i32x4.sub")]
    pub fn i32x4_sub(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i32x4_sub, i32, 4, as_i32x4, from_i32x4, wrapping_sub)
    }
    #[doc(alias = "i64x2.sub")]
    pub fn i64x2_sub(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i64x2_sub, i64, 2, as_i64x2, from_i64x2, wrapping_sub)
    }
    #[doc(alias = "i16x8.mul")]
    pub fn i16x8_mul(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i16x8_mul, i16, 8, as_i16x8, from_i16x8, wrapping_mul)
    }
    #[doc(alias = "i32x4.mul")]
    pub fn i32x4_mul(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i32x4_mul, i32, 4, as_i32x4, from_i32x4, wrapping_mul)
    }
    #[doc(alias = "i64x2.mul")]
    pub fn i64x2_mul(self, rhs: Self) -> Self {
        simd_wrapping_binop!(self, rhs, i64x2_mul, i64, 2, as_i64x2, from_i64x2, wrapping_mul)
    }

    #[doc(alias = "i8x16.add_sat_s")]
    pub fn i8x16_add_sat_s(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, i8x16_add_sat, i8, 16, as_i8x16, from_i8x16, saturating_add)
    }
    #[doc(alias = "i16x8.add_sat_s")]
    pub fn i16x8_add_sat_s(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, i16x8_add_sat, i16, 8, as_i16x8, from_i16x8, saturating_add)
    }
    #[doc(alias = "i8x16.add_sat_u")]
    pub fn i8x16_add_sat_u(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, u8x16_add_sat, u8, 16, as_u8x16, from_u8x16, saturating_add)
    }
    #[doc(alias = "i16x8.add_sat_u")]
    pub fn i16x8_add_sat_u(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, u16x8_add_sat, u16, 8, as_u16x8, from_u16x8, saturating_add)
    }
    #[doc(alias = "i8x16.sub_sat_s")]
    pub fn i8x16_sub_sat_s(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, i8x16_sub_sat, i8, 16, as_i8x16, from_i8x16, saturating_sub)
    }
    #[doc(alias = "i16x8.sub_sat_s")]
    pub fn i16x8_sub_sat_s(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, i16x8_sub_sat, i16, 8, as_i16x8, from_i16x8, saturating_sub)
    }
    #[doc(alias = "i8x16.sub_sat_u")]
    pub fn i8x16_sub_sat_u(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, u8x16_sub_sat, u8, 16, as_u8x16, from_u8x16, saturating_sub)
    }
    #[doc(alias = "i16x8.sub_sat_u")]
    pub fn i16x8_sub_sat_u(self, rhs: Self) -> Self {
        simd_sat_binop!(self, rhs, u16x8_sub_sat, u16, 8, as_u16x8, from_u16x8, saturating_sub)
    }

    #[doc(alias = "i8x16.avgr_u")]
    pub fn i8x16_avgr_u(self, rhs: Self) -> Self {
        simd_avgr_u!(self, rhs, u8x16_avgr, u8, u16, 16, as_u8x16, from_u8x16)
    }
    #[doc(alias = "i16x8.avgr_u")]
    pub fn i16x8_avgr_u(self, rhs: Self) -> Self {
        simd_avgr_u!(self, rhs, u16x8_avgr, u16, u32, 8, as_u16x8, from_u16x8)
    }

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

    #[doc(alias = "i16x8.extend_low_i8x16_s")]
    pub fn i16x8_extend_low_i8x16_s(self) -> Self {
        simd_extend_cast!(self, as_i8x16, from_i16x8, i16, 8, 0)
    }
    #[doc(alias = "i16x8.extend_low_i8x16_u")]
    pub fn i16x8_extend_low_i8x16_u(self) -> Self {
        simd_extend_cast!(self, as_u8x16, from_u16x8, u16, 8, 0)
    }
    #[doc(alias = "i16x8.extend_high_i8x16_s")]
    pub fn i16x8_extend_high_i8x16_s(self) -> Self {
        simd_extend_cast!(self, as_i8x16, from_i16x8, i16, 8, 8)
    }
    #[doc(alias = "i16x8.extend_high_i8x16_u")]
    pub fn i16x8_extend_high_i8x16_u(self) -> Self {
        simd_extend_cast!(self, as_u8x16, from_u16x8, u16, 8, 8)
    }
    #[doc(alias = "i32x4.extend_low_i16x8_s")]
    pub fn i32x4_extend_low_i16x8_s(self) -> Self {
        simd_extend_cast!(self, as_i16x8, from_i32x4, i32, 4, 0)
    }
    #[doc(alias = "i32x4.extend_low_i16x8_u")]
    pub fn i32x4_extend_low_i16x8_u(self) -> Self {
        simd_extend_cast!(self, as_u16x8, from_u32x4, u32, 4, 0)
    }
    #[doc(alias = "i32x4.extend_high_i16x8_s")]
    pub fn i32x4_extend_high_i16x8_s(self) -> Self {
        simd_extend_cast!(self, as_i16x8, from_i32x4, i32, 4, 4)
    }
    #[doc(alias = "i32x4.extend_high_i16x8_u")]
    pub fn i32x4_extend_high_i16x8_u(self) -> Self {
        simd_extend_cast!(self, as_u16x8, from_u32x4, u32, 4, 4)
    }
    #[doc(alias = "i64x2.extend_low_i32x4_s")]
    pub fn i64x2_extend_low_i32x4_s(self) -> Self {
        simd_extend_cast!(self, as_i32x4, from_i64x2, i64, 2, 0)
    }
    #[doc(alias = "i64x2.extend_low_i32x4_u")]
    pub fn i64x2_extend_low_i32x4_u(self) -> Self {
        simd_extend_cast!(self, as_u32x4, from_u64x2, u64, 2, 0)
    }
    #[doc(alias = "i64x2.extend_high_i32x4_s")]
    pub fn i64x2_extend_high_i32x4_s(self) -> Self {
        simd_extend_cast!(self, as_i32x4, from_i64x2, i64, 2, 2)
    }
    #[doc(alias = "i64x2.extend_high_i32x4_u")]
    pub fn i64x2_extend_high_i32x4_u(self) -> Self {
        simd_extend_cast!(self, as_u32x4, from_u64x2, u64, 2, 2)
    }

    #[doc(alias = "i16x8.extmul_low_i8x16_s")]
    pub fn i16x8_extmul_low_i8x16_s(self, rhs: Self) -> Self {
        simd_extmul_signed!(self, rhs, as_i8x16, from_i16x8, i16, 8, 0)
    }
    #[doc(alias = "i16x8.extmul_low_i8x16_u")]
    pub fn i16x8_extmul_low_i8x16_u(self, rhs: Self) -> Self {
        simd_extmul_unsigned!(self, rhs, as_u8x16, from_u16x8, u16, 8, 0)
    }
    #[doc(alias = "i16x8.extmul_high_i8x16_s")]
    pub fn i16x8_extmul_high_i8x16_s(self, rhs: Self) -> Self {
        simd_extmul_signed!(self, rhs, as_i8x16, from_i16x8, i16, 8, 8)
    }
    #[doc(alias = "i16x8.extmul_high_i8x16_u")]
    pub fn i16x8_extmul_high_i8x16_u(self, rhs: Self) -> Self {
        simd_extmul_unsigned!(self, rhs, as_u8x16, from_u16x8, u16, 8, 8)
    }
    #[doc(alias = "i32x4.extmul_low_i16x8_s")]
    pub fn i32x4_extmul_low_i16x8_s(self, rhs: Self) -> Self {
        simd_extmul_signed!(self, rhs, as_i16x8, from_i32x4, i32, 4, 0)
    }
    #[doc(alias = "i32x4.extmul_low_i16x8_u")]
    pub fn i32x4_extmul_low_i16x8_u(self, rhs: Self) -> Self {
        simd_extmul_unsigned!(self, rhs, as_u16x8, from_u32x4, u32, 4, 0)
    }
    #[doc(alias = "i32x4.extmul_high_i16x8_s")]
    pub fn i32x4_extmul_high_i16x8_s(self, rhs: Self) -> Self {
        simd_extmul_signed!(self, rhs, as_i16x8, from_i32x4, i32, 4, 4)
    }
    #[doc(alias = "i32x4.extmul_high_i16x8_u")]
    pub fn i32x4_extmul_high_i16x8_u(self, rhs: Self) -> Self {
        simd_extmul_unsigned!(self, rhs, as_u16x8, from_u32x4, u32, 4, 4)
    }
    #[doc(alias = "i64x2.extmul_low_i32x4_s")]
    pub fn i64x2_extmul_low_i32x4_s(self, rhs: Self) -> Self {
        simd_extmul_signed!(self, rhs, as_i32x4, from_i64x2, i64, 2, 0)
    }
    #[doc(alias = "i64x2.extmul_low_i32x4_u")]
    pub fn i64x2_extmul_low_i32x4_u(self, rhs: Self) -> Self {
        simd_extmul_unsigned!(self, rhs, as_u32x4, from_u64x2, u64, 2, 0)
    }
    #[doc(alias = "i64x2.extmul_high_i32x4_s")]
    pub fn i64x2_extmul_high_i32x4_s(self, rhs: Self) -> Self {
        simd_extmul_signed!(self, rhs, as_i32x4, from_i64x2, i64, 2, 2)
    }
    #[doc(alias = "i64x2.extmul_high_i32x4_u")]
    pub fn i64x2_extmul_high_i32x4_u(self, rhs: Self) -> Self {
        simd_extmul_unsigned!(self, rhs, as_u32x4, from_u64x2, u64, 2, 2)
    }

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

    #[doc(alias = "i8x16.relaxed_laneselect")]
    pub fn i8x16_relaxed_laneselect(v1: Self, v2: Self, c: Self) -> Self {
        Self::v128_bitselect(v1, v2, c)
    }

    #[doc(alias = "i16x8.relaxed_laneselect")]
    pub fn i16x8_relaxed_laneselect(v1: Self, v2: Self, c: Self) -> Self {
        Self::v128_bitselect(v1, v2, c)
    }

    #[doc(alias = "i32x4.relaxed_laneselect")]
    pub fn i32x4_relaxed_laneselect(v1: Self, v2: Self, c: Self) -> Self {
        Self::v128_bitselect(v1, v2, c)
    }

    #[doc(alias = "i64x2.relaxed_laneselect")]
    pub fn i64x2_relaxed_laneselect(v1: Self, v2: Self, c: Self) -> Self {
        Self::v128_bitselect(v1, v2, c)
    }

    #[doc(alias = "i16x8.relaxed_q15mulr_s")]
    pub fn i16x8_relaxed_q15mulr_s(self, rhs: Self) -> Self {
        self.i16x8_q15mulr_sat_s(rhs)
    }

    #[doc(alias = "i16x8.relaxed_dot_i8x16_i7x16_s")]
    pub fn i16x8_relaxed_dot_i8x16_i7x16_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let mut out = [0i16; 8];

        for (dst, (a_pair, b_pair)) in out.iter_mut().zip(a.chunks_exact(2).zip(b.chunks_exact(2))) {
            let prod0 = (a_pair[0] as i16) * (b_pair[0] as i16);
            let prod1 = (a_pair[1] as i16) * (b_pair[1] as i16);
            *dst = prod0.wrapping_add(prod1);
        }

        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.relaxed_dot_i8x16_i7x16_add_s")]
    pub fn i32x4_relaxed_dot_i8x16_i7x16_add_s(self, rhs: Self, acc: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let c = acc.as_i32x4();
        let mut out = [0i32; 4];

        for (i, dst) in out.iter_mut().enumerate() {
            let base = i * 4;
            let mut sum = 0i32;
            for j in 0..4 {
                sum = sum.wrapping_add((a[base + j] as i32).wrapping_mul(b[base + j] as i32));
            }
            *dst = sum.wrapping_add(c[i]);
        }

        Self::from_i32x4(out)
    }

    #[doc(alias = "i8x16.eq")]
    pub fn i8x16_eq(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i8x16_eq, i8, 16, as_i8x16, from_i8x16, ==)
    }
    #[doc(alias = "i16x8.eq")]
    pub fn i16x8_eq(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i16x8_eq, i16, 8, as_i16x8, from_i16x8, ==)
    }
    #[doc(alias = "i32x4.eq")]
    pub fn i32x4_eq(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i32x4_eq, i32, 4, as_i32x4, from_i32x4, ==)
    }
    #[doc(alias = "i64x2.eq")]
    pub fn i64x2_eq(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i64x2_eq, i64, 2, as_i64x2, from_i64x2, ==)
    }
    #[doc(alias = "i8x16.ne")]
    pub fn i8x16_ne(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i8x16_ne, i8, 16, as_i8x16, from_i8x16, !=)
    }
    #[doc(alias = "i16x8.ne")]
    pub fn i16x8_ne(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i16x8_ne, i16, 8, as_i16x8, from_i16x8, !=)
    }
    #[doc(alias = "i32x4.ne")]
    pub fn i32x4_ne(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i32x4_ne, i32, 4, as_i32x4, from_i32x4, !=)
    }
    #[doc(alias = "i64x2.ne")]
    pub fn i64x2_ne(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i64x2_ne, i64, 2, as_i64x2, from_i64x2, !=)
    }
    #[doc(alias = "i8x16.lt_s")]
    pub fn i8x16_lt_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i8x16_lt, i8, 16, as_i8x16, from_i8x16, <)
    }
    #[doc(alias = "i16x8.lt_s")]
    pub fn i16x8_lt_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i16x8_lt, i16, 8, as_i16x8, from_i16x8, <)
    }
    #[doc(alias = "i32x4.lt_s")]
    pub fn i32x4_lt_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i32x4_lt, i32, 4, as_i32x4, from_i32x4, <)
    }
    #[doc(alias = "i64x2.lt_s")]
    pub fn i64x2_lt_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i64x2_lt, i64, 2, as_i64x2, from_i64x2, <)
    }
    #[doc(alias = "i8x16.lt_u")]
    pub fn i8x16_lt_u(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, u8x16_lt, i8, 16, as_u8x16, from_i8x16, <)
    }
    #[doc(alias = "i16x8.lt_u")]
    pub fn i16x8_lt_u(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, u16x8_lt, i16, 8, as_u16x8, from_i16x8, <)
    }
    #[doc(alias = "i32x4.lt_u")]
    pub fn i32x4_lt_u(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, u32x4_lt, i32, 4, as_u32x4, from_i32x4, <)
    }

    #[doc(alias = "i8x16.gt_s")]
    pub fn i8x16_gt_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i8x16_lt_s)
    }
    #[doc(alias = "i16x8.gt_s")]
    pub fn i16x8_gt_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i16x8_lt_s)
    }
    #[doc(alias = "i32x4.gt_s")]
    pub fn i32x4_gt_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i32x4_lt_s)
    }
    #[doc(alias = "i64x2.gt_s")]
    pub fn i64x2_gt_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i64x2_lt_s)
    }
    #[doc(alias = "i8x16.gt_u")]
    pub fn i8x16_gt_u(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i8x16_lt_u)
    }
    #[doc(alias = "i16x8.gt_u")]
    pub fn i16x8_gt_u(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i16x8_lt_u)
    }
    #[doc(alias = "i32x4.gt_u")]
    pub fn i32x4_gt_u(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i32x4_lt_u)
    }
    #[doc(alias = "i8x16.le_s")]
    pub fn i8x16_le_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i8x16_ge_s)
    }
    #[doc(alias = "i16x8.le_s")]
    pub fn i16x8_le_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i16x8_ge_s)
    }
    #[doc(alias = "i32x4.le_s")]
    pub fn i32x4_le_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i32x4_ge_s)
    }
    #[doc(alias = "i64x2.le_s")]
    pub fn i64x2_le_s(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i64x2_ge_s)
    }
    #[doc(alias = "i8x16.le_u")]
    pub fn i8x16_le_u(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i8x16_ge_u)
    }
    #[doc(alias = "i16x8.le_u")]
    pub fn i16x8_le_u(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i16x8_ge_u)
    }
    #[doc(alias = "i32x4.le_u")]
    pub fn i32x4_le_u(self, rhs: Self) -> Self {
        simd_cmp_delegate!(self, rhs, i32x4_ge_u)
    }

    #[doc(alias = "i8x16.ge_s")]
    pub fn i8x16_ge_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i8x16_ge, i8, 16, as_i8x16, from_i8x16, >=)
    }
    #[doc(alias = "i16x8.ge_s")]
    pub fn i16x8_ge_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i16x8_ge, i16, 8, as_i16x8, from_i16x8, >=)
    }
    #[doc(alias = "i32x4.ge_s")]
    pub fn i32x4_ge_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i32x4_ge, i32, 4, as_i32x4, from_i32x4, >=)
    }
    #[doc(alias = "i64x2.ge_s")]
    pub fn i64x2_ge_s(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, i64x2_ge, i64, 2, as_i64x2, from_i64x2, >=)
    }
    #[doc(alias = "i8x16.ge_u")]
    pub fn i8x16_ge_u(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, u8x16_ge, i8, 16, as_u8x16, from_i8x16, >=)
    }
    #[doc(alias = "i16x8.ge_u")]
    pub fn i16x8_ge_u(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, u16x8_ge, i16, 8, as_u16x8, from_i16x8, >=)
    }
    #[doc(alias = "i32x4.ge_u")]
    pub fn i32x4_ge_u(self, rhs: Self) -> Self {
        simd_cmp_mask!(self, rhs, u32x4_ge, i32, 4, as_u32x4, from_i32x4, >=)
    }

    #[doc(alias = "i8x16.abs")]
    pub fn i8x16_abs(self) -> Self {
        simd_abs_const!(self, i8, 16, as_i8x16, from_i8x16)
    }
    #[doc(alias = "i16x8.abs")]
    pub fn i16x8_abs(self) -> Self {
        simd_abs_const!(self, i16, 8, as_i16x8, from_i16x8)
    }
    #[doc(alias = "i32x4.abs")]
    pub fn i32x4_abs(self) -> Self {
        simd_abs_const!(self, i32, 4, as_i32x4, from_i32x4)
    }
    #[doc(alias = "i64x2.abs")]
    pub fn i64x2_abs(self) -> Self {
        simd_abs_const!(self, i64, 2, as_i64x2, from_i64x2)
    }

    #[doc(alias = "i8x16.neg")]
    pub fn i8x16_neg(self) -> Self {
        simd_neg!(self, i8x16_neg, i8, 16, as_i8x16, from_i8x16)
    }
    #[doc(alias = "i16x8.neg")]
    pub fn i16x8_neg(self) -> Self {
        simd_neg!(self, i16x8_neg, i16, 8, as_i16x8, from_i16x8)
    }
    #[doc(alias = "i32x4.neg")]
    pub fn i32x4_neg(self) -> Self {
        simd_neg!(self, i32x4_neg, i32, 4, as_i32x4, from_i32x4)
    }
    #[doc(alias = "i64x2.neg")]
    pub fn i64x2_neg(self) -> Self {
        simd_neg!(self, i64x2_neg, i64, 2, as_i64x2, from_i64x2)
    }

    #[doc(alias = "i8x16.min_s")]
    pub fn i8x16_min_s(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, i8x16_min, i8, 16, as_i8x16, from_i8x16, <)
    }
    #[doc(alias = "i16x8.min_s")]
    pub fn i16x8_min_s(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, i16x8_min, i16, 8, as_i16x8, from_i16x8, <)
    }
    #[doc(alias = "i32x4.min_s")]
    pub fn i32x4_min_s(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, i32x4_min, i32, 4, as_i32x4, from_i32x4, <)
    }
    #[doc(alias = "i8x16.min_u")]
    pub fn i8x16_min_u(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, u8x16_min, u8, 16, as_u8x16, from_u8x16, <)
    }
    #[doc(alias = "i16x8.min_u")]
    pub fn i16x8_min_u(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, u16x8_min, u16, 8, as_u16x8, from_u16x8, <)
    }
    #[doc(alias = "i32x4.min_u")]
    pub fn i32x4_min_u(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, u32x4_min, u32, 4, as_u32x4, from_u32x4, <)
    }
    #[doc(alias = "i8x16.max_s")]
    pub fn i8x16_max_s(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, i8x16_max, i8, 16, as_i8x16, from_i8x16, >)
    }
    #[doc(alias = "i16x8.max_s")]
    pub fn i16x8_max_s(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, i16x8_max, i16, 8, as_i16x8, from_i16x8, >)
    }
    #[doc(alias = "i32x4.max_s")]
    pub fn i32x4_max_s(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, i32x4_max, i32, 4, as_i32x4, from_i32x4, >)
    }
    #[doc(alias = "i8x16.max_u")]
    pub fn i8x16_max_u(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, u8x16_max, u8, 16, as_u8x16, from_u8x16, >)
    }
    #[doc(alias = "i16x8.max_u")]
    pub fn i16x8_max_u(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, u16x8_max, u16, 8, as_u16x8, from_u16x8, >)
    }
    #[doc(alias = "i32x4.max_u")]
    pub fn i32x4_max_u(self, rhs: Self) -> Self {
        simd_minmax!(self, rhs, u32x4_max, u32, 4, as_u32x4, from_u32x4, >)
    }

    #[doc(alias = "f32x4.eq")]
    pub fn f32x4_eq(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i32, 4, as_f32x4, from_i32x4, ==)
    }
    #[doc(alias = "f64x2.eq")]
    pub fn f64x2_eq(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i64, 2, as_f64x2, from_i64x2, ==)
    }
    #[doc(alias = "f32x4.ne")]
    pub fn f32x4_ne(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i32, 4, as_f32x4, from_i32x4, !=)
    }
    #[doc(alias = "f64x2.ne")]
    pub fn f64x2_ne(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i64, 2, as_f64x2, from_i64x2, !=)
    }
    #[doc(alias = "f32x4.lt")]
    pub fn f32x4_lt(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i32, 4, as_f32x4, from_i32x4, <)
    }
    #[doc(alias = "f64x2.lt")]
    pub fn f64x2_lt(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i64, 2, as_f64x2, from_i64x2, <)
    }

    #[doc(alias = "f32x4.gt")]
    pub fn f32x4_gt(self, rhs: Self) -> Self {
        rhs.f32x4_lt(self)
    }

    #[doc(alias = "f64x2.gt")]
    pub fn f64x2_gt(self, rhs: Self) -> Self {
        rhs.f64x2_lt(self)
    }

    #[doc(alias = "f32x4.le")]
    pub fn f32x4_le(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i32, 4, as_f32x4, from_i32x4, <=)
    }
    #[doc(alias = "f64x2.le")]
    pub fn f64x2_le(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i64, 2, as_f64x2, from_i64x2, <=)
    }
    #[doc(alias = "f32x4.ge")]
    pub fn f32x4_ge(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i32, 4, as_f32x4, from_i32x4, >=)
    }
    #[doc(alias = "f64x2.ge")]
    pub fn f64x2_ge(self, rhs: Self) -> Self {
        simd_cmp_mask_const!(self, rhs, i64, 2, as_f64x2, from_i64x2, >=)
    }

    #[doc(alias = "f32x4.ceil")]
    pub fn f32x4_ceil(self) -> Self {
        simd_float_unary!(self, map_f32x4, |x| canonicalize_simd_f32_nan(x.ceil()))
    }
    #[doc(alias = "f64x2.ceil")]
    pub fn f64x2_ceil(self) -> Self {
        simd_float_unary!(self, map_f64x2, |x| canonicalize_simd_f64_nan(x.ceil()))
    }
    #[doc(alias = "f32x4.floor")]
    pub fn f32x4_floor(self) -> Self {
        simd_float_unary!(self, map_f32x4, |x| canonicalize_simd_f32_nan(x.floor()))
    }
    #[doc(alias = "f64x2.floor")]
    pub fn f64x2_floor(self) -> Self {
        simd_float_unary!(self, map_f64x2, |x| canonicalize_simd_f64_nan(x.floor()))
    }
    #[doc(alias = "f32x4.trunc")]
    pub fn f32x4_trunc(self) -> Self {
        simd_float_unary!(self, map_f32x4, |x| canonicalize_simd_f32_nan(x.trunc()))
    }
    #[doc(alias = "f64x2.trunc")]
    pub fn f64x2_trunc(self) -> Self {
        simd_float_unary!(self, map_f64x2, |x| canonicalize_simd_f64_nan(x.trunc()))
    }
    #[doc(alias = "f32x4.nearest")]
    pub fn f32x4_nearest(self) -> Self {
        simd_float_unary!(self, map_f32x4, |x| canonicalize_simd_f32_nan(TinywasmFloatExt::tw_nearest(x)))
    }
    #[doc(alias = "f64x2.nearest")]
    pub fn f64x2_nearest(self) -> Self {
        simd_float_unary!(self, map_f64x2, |x| canonicalize_simd_f64_nan(TinywasmFloatExt::tw_nearest(x)))
    }
    #[doc(alias = "f32x4.abs")]
    pub fn f32x4_abs(self) -> Self {
        simd_float_unary!(self, map_f32x4, f32::abs)
    }
    #[doc(alias = "f64x2.abs")]
    pub fn f64x2_abs(self) -> Self {
        simd_float_unary!(self, map_f64x2, f64::abs)
    }
    #[doc(alias = "f32x4.neg")]
    pub fn f32x4_neg(self) -> Self {
        simd_float_unary!(self, map_f32x4, |x| -x)
    }
    #[doc(alias = "f64x2.neg")]
    pub fn f64x2_neg(self) -> Self {
        simd_float_unary!(self, map_f64x2, |x| -x)
    }
    #[doc(alias = "f32x4.sqrt")]
    pub fn f32x4_sqrt(self) -> Self {
        simd_float_unary!(self, map_f32x4, |x| canonicalize_simd_f32_nan(x.sqrt()))
    }
    #[doc(alias = "f64x2.sqrt")]
    pub fn f64x2_sqrt(self) -> Self {
        simd_float_unary!(self, map_f64x2, |x| canonicalize_simd_f64_nan(x.sqrt()))
    }

    #[doc(alias = "f32x4.add")]
    pub fn f32x4_add(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, |a, b| canonicalize_simd_f32_nan(a + b))
    }
    #[doc(alias = "f64x2.add")]
    pub fn f64x2_add(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, |a, b| canonicalize_simd_f64_nan(a + b))
    }
    #[doc(alias = "f32x4.sub")]
    pub fn f32x4_sub(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, |a, b| canonicalize_simd_f32_nan(a - b))
    }
    #[doc(alias = "f64x2.sub")]
    pub fn f64x2_sub(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, |a, b| canonicalize_simd_f64_nan(a - b))
    }
    #[doc(alias = "f32x4.mul")]
    pub fn f32x4_mul(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, |a, b| canonicalize_simd_f32_nan(a * b))
    }
    #[doc(alias = "f64x2.mul")]
    pub fn f64x2_mul(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, |a, b| canonicalize_simd_f64_nan(a * b))
    }
    #[doc(alias = "f32x4.div")]
    pub fn f32x4_div(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, |a, b| canonicalize_simd_f32_nan(a / b))
    }
    #[doc(alias = "f64x2.div")]
    pub fn f64x2_div(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, |a, b| canonicalize_simd_f64_nan(a / b))
    }
    #[doc(alias = "f32x4.min")]
    pub fn f32x4_min(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, TinywasmFloatExt::tw_minimum)
    }
    #[doc(alias = "f64x2.min")]
    pub fn f64x2_min(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, TinywasmFloatExt::tw_minimum)
    }
    #[doc(alias = "f32x4.max")]
    pub fn f32x4_max(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, TinywasmFloatExt::tw_maximum)
    }
    #[doc(alias = "f64x2.max")]
    pub fn f64x2_max(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, TinywasmFloatExt::tw_maximum)
    }
    #[doc(alias = "f32x4.pmin")]
    pub fn f32x4_pmin(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, |a, b| if b < a { b } else { a })
    }
    #[doc(alias = "f64x2.pmin")]
    pub fn f64x2_pmin(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, |a, b| if b < a { b } else { a })
    }
    #[doc(alias = "f32x4.pmax")]
    pub fn f32x4_pmax(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f32x4, |a, b| if b > a { b } else { a })
    }
    #[doc(alias = "f64x2.pmax")]
    pub fn f64x2_pmax(self, rhs: Self) -> Self {
        simd_float_binary!(self, rhs, zip_f64x2, |a, b| if b > a { b } else { a })
    }

    #[doc(alias = "f32x4.relaxed_madd")]
    pub fn f32x4_relaxed_madd(self, b: Self, c: Self) -> Self {
        self.zip_f32x4(b, |x, y| canonicalize_simd_f32_nan(x * y))
            .zip_f32x4(c, |xy, z| canonicalize_simd_f32_nan(xy + z))
    }

    #[doc(alias = "f32x4.relaxed_nmadd")]
    pub fn f32x4_relaxed_nmadd(self, b: Self, c: Self) -> Self {
        self.zip_f32x4(b, |x, y| canonicalize_simd_f32_nan(-(x * y)))
            .zip_f32x4(c, |neg_xy, z| canonicalize_simd_f32_nan(neg_xy + z))
    }

    #[doc(alias = "f64x2.relaxed_madd")]
    pub fn f64x2_relaxed_madd(self, b: Self, c: Self) -> Self {
        self.zip_f64x2(b, |x, y| canonicalize_simd_f64_nan(x * y))
            .zip_f64x2(c, |xy, z| canonicalize_simd_f64_nan(xy + z))
    }

    #[doc(alias = "f64x2.relaxed_nmadd")]
    pub fn f64x2_relaxed_nmadd(self, b: Self, c: Self) -> Self {
        self.zip_f64x2(b, |x, y| canonicalize_simd_f64_nan(-(x * y)))
            .zip_f64x2(c, |neg_xy, z| canonicalize_simd_f64_nan(neg_xy + z))
    }

    #[doc(alias = "f32x4.relaxed_min")]
    pub fn f32x4_relaxed_min(self, rhs: Self) -> Self {
        self.f32x4_min(rhs)
    }

    #[doc(alias = "f64x2.relaxed_min")]
    pub fn f64x2_relaxed_min(self, rhs: Self) -> Self {
        self.f64x2_min(rhs)
    }

    #[doc(alias = "f32x4.relaxed_max")]
    pub fn f32x4_relaxed_max(self, rhs: Self) -> Self {
        self.f32x4_max(rhs)
    }

    #[doc(alias = "f64x2.relaxed_max")]
    pub fn f64x2_relaxed_max(self, rhs: Self) -> Self {
        self.f64x2_max(rhs)
    }

    #[doc(alias = "i32x4.relaxed_trunc_f32x4_s")]
    pub fn i32x4_relaxed_trunc_f32x4_s(self) -> Self {
        self.i32x4_trunc_sat_f32x4_s()
    }

    #[doc(alias = "i32x4.relaxed_trunc_f32x4_u")]
    pub fn i32x4_relaxed_trunc_f32x4_u(self) -> Self {
        self.i32x4_trunc_sat_f32x4_u()
    }

    #[doc(alias = "i32x4.relaxed_trunc_f64x2_s_zero")]
    pub fn i32x4_relaxed_trunc_f64x2_s_zero(self) -> Self {
        self.i32x4_trunc_sat_f64x2_s_zero()
    }

    #[doc(alias = "i32x4.relaxed_trunc_f64x2_u_zero")]
    pub fn i32x4_relaxed_trunc_f64x2_u_zero(self) -> Self {
        self.i32x4_trunc_sat_f64x2_u_zero()
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

    #[doc(alias = "i16x8.splat")]
    pub fn splat_i16(src: i16) -> Self {
        Self::from_i16x8([src; 8])
    }

    #[doc(alias = "i32x4.splat")]
    pub fn splat_i32(src: i32) -> Self {
        Self::from_i32x4([src; 4])
    }

    #[doc(alias = "i64x2.splat")]
    pub fn splat_i64(src: i64) -> Self {
        Self::from_i64x2([src; 2])
    }

    #[doc(alias = "f32x4.splat")]
    pub fn splat_f32(src: f32) -> Self {
        Self::splat_i32(src.to_bits() as i32)
    }

    #[doc(alias = "f64x2.splat")]
    pub fn splat_f64(src: f64) -> Self {
        Self::splat_i64(src.to_bits() as i64)
    }

    #[doc(alias = "i8x16.extract_lane_s")]
    pub fn extract_lane_i8(self, lane: u8) -> i8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane] as i8
    }

    #[doc(alias = "i8x16.extract_lane_u")]
    pub fn extract_lane_u8(self, lane: u8) -> u8 {
        debug_assert!(lane < 16);
        let lane = lane as usize;
        let bytes = self.to_le_bytes();
        bytes[lane]
    }

    #[doc(alias = "i16x8.extract_lane_s")]
    pub fn extract_lane_i16(self, lane: u8) -> i16 {
        i16::from_le_bytes(self.extract_lane_bytes::<2>(lane, 8))
    }

    #[doc(alias = "i16x8.extract_lane_u")]
    pub fn extract_lane_u16(self, lane: u8) -> u16 {
        u16::from_le_bytes(self.extract_lane_bytes::<2>(lane, 8))
    }

    #[doc(alias = "i32x4.extract_lane")]
    pub fn extract_lane_i32(self, lane: u8) -> i32 {
        i32::from_le_bytes(self.extract_lane_bytes::<4>(lane, 4))
    }

    #[doc(alias = "i64x2.extract_lane")]
    pub fn extract_lane_i64(self, lane: u8) -> i64 {
        i64::from_le_bytes(self.extract_lane_bytes::<8>(lane, 2))
    }

    #[doc(alias = "f32x4.extract_lane")]
    pub fn extract_lane_f32(self, lane: u8) -> f32 {
        f32::from_bits(self.extract_lane_i32(lane) as u32)
    }

    #[doc(alias = "f64x2.extract_lane")]
    pub fn extract_lane_f64(self, lane: u8) -> f64 {
        f64::from_bits(self.extract_lane_i64(lane) as u64)
    }
}
