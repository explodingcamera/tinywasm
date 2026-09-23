use super::super::num_helpers::TinywasmFloatExt;
use super::Value128;
use super::utils::*;
use core::array;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
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
    pub(crate) fn v128_any_true(self) -> bool {
        simd_impl! {
            wasm => { wasm::v128_any_true(self.to_wasm_v128()) }
            generic => { self.0.iter().any(|&b| b != 0) }
        }
    }

    #[doc(alias = "v128.not")]
    pub(crate) fn v128_not(self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_not(self.to_wasm_v128())) }
            generic => { Self(self.0.map(|b| !b)) }
        }
    }

    #[doc(alias = "v128.and")]
    pub(crate) fn v128_and(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_and(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { (i128::from(self) & i128::from(rhs)).into() }
        }
    }

    #[doc(alias = "v128.andnot")]
    pub(crate) fn v128_andnot(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_andnot(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { (i128::from(self) & !i128::from(rhs)).into() }
        }
    }

    #[doc(alias = "v128.or")]
    pub(crate) fn v128_or(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_or(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { (i128::from(self) | i128::from(rhs)).into() }
        }
    }

    #[doc(alias = "v128.xor")]
    pub(crate) fn v128_xor(self, rhs: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_xor(self.to_wasm_v128(), rhs.to_wasm_v128())) }
            generic => { (i128::from(self) ^ i128::from(rhs)).into() }
        }
    }

    #[doc(alias = "v128.bitselect")]
    pub(crate) fn v128_bitselect(v1: Self, v2: Self, c: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::v128_bitselect(v1.to_wasm_v128(), v2.to_wasm_v128(), c.to_wasm_v128())) }
            generic => { ((i128::from(v1) & i128::from(c)) | (i128::from(v2) & !i128::from(c))).into() }
        }
    }

    #[doc(alias = "v128.load8x8_s")]
    pub(crate) const fn v128_load8x8_s(src: [u8; 8]) -> Self {
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
    pub(crate) const fn v128_load8x8_u(src: [u8; 8]) -> Self {
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
    pub(crate) const fn v128_load16x4_s(src: [u8; 8]) -> Self {
        Self::from_i32x4([
            i16::from_le_bytes([src[0], src[1]]) as i32,
            i16::from_le_bytes([src[2], src[3]]) as i32,
            i16::from_le_bytes([src[4], src[5]]) as i32,
            i16::from_le_bytes([src[6], src[7]]) as i32,
        ])
    }

    #[doc(alias = "v128.load16x4_u")]
    pub(crate) const fn v128_load16x4_u(src: [u8; 8]) -> Self {
        Self::from_u32x4([
            u16::from_le_bytes([src[0], src[1]]) as u32,
            u16::from_le_bytes([src[2], src[3]]) as u32,
            u16::from_le_bytes([src[4], src[5]]) as u32,
            u16::from_le_bytes([src[6], src[7]]) as u32,
        ])
    }

    #[doc(alias = "v128.load32x2_s")]
    pub(crate) const fn v128_load32x2_s(src: [u8; 8]) -> Self {
        Self::from_i64x2([
            i32::from_le_bytes([src[0], src[1], src[2], src[3]]) as i64,
            i32::from_le_bytes([src[4], src[5], src[6], src[7]]) as i64,
        ])
    }

    #[doc(alias = "v128.load32x2_u")]
    pub(crate) const fn v128_load32x2_u(src: [u8; 8]) -> Self {
        Self::from_u64x2([
            u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as u64,
            u32::from_le_bytes([src[4], src[5], src[6], src[7]]) as u64,
        ])
    }

    #[doc(alias = "i8x16.swizzle")]
    pub(crate) fn i8x16_swizzle(self, s: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::i8x16_swizzle(self.to_wasm_v128(), s.to_wasm_v128())) }
            x86 => {
                let a = self.0;
                let idx = s.0;
                let mask = idx.map(|j| if j < 16 { j & 0x0f } else { 0x80 });

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
                Self(out)
            }
            generic => {
                let a = self.0;
                let idx = s.0;
                let mut out = [0u8; 16];
                // Keep this as a manual loop: array::from_fn generates worse x86 asm here.
                for i in 0..16 {
                    let j = idx[i];
                    let lane = a[(j & 0x0f) as usize];
                    out[i] = if j < 16 { lane } else { 0 };
                }
                Self(out)
            }
        }
    }

    #[doc(alias = "i8x16.relaxed_swizzle")]
    pub(crate) fn i8x16_relaxed_swizzle(self, s: Self) -> Self {
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::i8x16_relaxed_swizzle(self.to_wasm_v128(), s.to_wasm_v128())) }
            generic => { self.i8x16_swizzle(s) }
        }
    }

    #[doc(alias = "i8x16.shuffle")]
    pub(crate) fn i8x16_shuffle(a: Self, b: Self, idx: Self) -> Self {
        simd_impl! {
            x86 => {
                let idx = idx.0;
                let mask_a = idx.map(|j| {
                    let j = j & 31;
                    if j < 16 { j } else { 0x80 }
                });
                let mask_b = idx.map(|j| {
                    let j = j & 31;
                    if j < 16 { 0x80 } else { j & 0x0f }
                });

                // SAFETY: all inputs are valid 16-byte buffers, and `_mm_loadu/_mm_storeu` support unaligned accesses.
                #[allow(unsafe_code)]
                let out = unsafe {
                    let a_vec = x86::_mm_loadu_si128(a.0.as_ptr().cast::<x86::__m128i>());
                    let b_vec = x86::_mm_loadu_si128(b.0.as_ptr().cast::<x86::__m128i>());
                    let mask_a_vec = x86::_mm_loadu_si128(mask_a.as_ptr().cast::<x86::__m128i>());
                    let mask_b_vec = x86::_mm_loadu_si128(mask_b.as_ptr().cast::<x86::__m128i>());
                    let a_part = x86::_mm_shuffle_epi8(a_vec, mask_a_vec);
                    let b_part = x86::_mm_shuffle_epi8(b_vec, mask_b_vec);
                    let result = x86::_mm_or_si128(a_part, b_part);
                    let mut out = [0u8; 16];
                    x86::_mm_storeu_si128(out.as_mut_ptr().cast::<x86::__m128i>(), result);
                    out
                };
                Self(out)
            }
            generic => {
                let a_bytes = a.0;
                let b_bytes = b.0;
                let idx = idx.0;
                let mut out = [0u8; 16];
                // Keep this as a manual loop: array::from_fn generates worse x86 asm here.
                for i in 0..16 {
                    let j = idx[i] & 31;
                    out[i] = if j < 16 { a_bytes[j as usize] } else { b_bytes[(j & 0x0f) as usize] };
                }
                Self(out)
            }
        }
    }

    #[doc(alias = "i8x16.bitmask")]
    pub(crate) fn i8x16_bitmask(self) -> u32 {
        let bytes = self.0;
        let mut mask = 0u32;
        for (i, byte) in bytes.into_iter().enumerate() {
            if (byte & 0x80) != 0 {
                mask |= 1u32 << i;
            }
        }
        mask
    }

    #[doc(alias = "i16x8.bitmask")]
    pub(crate) fn i16x8_bitmask(self) -> u32 {
        let bytes = self.0;
        let mut mask = 0u32;
        for (i, lane) in bytes.as_chunks::<2>().0.iter().enumerate() {
            if (lane[1] & 0x80) != 0 {
                mask |= 1u32 << i;
            }
        }
        mask
    }

    #[doc(alias = "i32x4.bitmask")]
    pub(crate) fn i32x4_bitmask(self) -> u32 {
        let bytes = self.0;
        let mut mask = 0u32;
        for (i, lane) in bytes.as_chunks::<4>().0.iter().enumerate() {
            if (lane[3] & 0x80) != 0 {
                mask |= 1u32 << i;
            }
        }
        mask
    }

    #[doc(alias = "i64x2.bitmask")]
    pub(crate) fn i64x2_bitmask(self) -> u32 {
        let x = u128::from_le_bytes(self.0);
        (((x >> 63) & 1) as u32) | ((((x >> 127) & 1) as u32) << 1)
    }

    #[doc(alias = "i8x16.narrow_i16x8_s")]
    pub(crate) fn i8x16_narrow_i16x8_s(a: Self, b: Self) -> Self {
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
    pub(crate) fn i8x16_narrow_i16x8_u(a: Self, b: Self) -> Self {
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
    pub(crate) fn i16x8_narrow_i32x4_s(a: Self, b: Self) -> Self {
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
    pub(crate) fn i16x8_narrow_i32x4_u(a: Self, b: Self) -> Self {
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
    pub(crate) fn i16x8_extadd_pairwise_i8x16_s(self) -> Self {
        let lanes = self.as_i8x16();
        Self::from_i16x8(array::from_fn(|i| lanes[i * 2] as i16 + lanes[i * 2 + 1] as i16))
    }

    #[doc(alias = "i16x8.extadd_pairwise_i8x16_u")]
    pub(crate) fn i16x8_extadd_pairwise_i8x16_u(self) -> Self {
        let lanes = self.as_u8x16();
        Self::from_u16x8(array::from_fn(|i| lanes[i * 2] as u16 + lanes[i * 2 + 1] as u16))
    }

    #[doc(alias = "i32x4.extadd_pairwise_i16x8_s")]
    pub(crate) fn i32x4_extadd_pairwise_i16x8_s(self) -> Self {
        let lanes = self.as_i16x8();
        Self::from_i32x4(array::from_fn(|i| lanes[i * 2] as i32 + lanes[i * 2 + 1] as i32))
    }

    #[doc(alias = "i32x4.extadd_pairwise_i16x8_u")]
    pub(crate) fn i32x4_extadd_pairwise_i16x8_u(self) -> Self {
        let lanes = self.as_u16x8();
        Self::from_u32x4(array::from_fn(|i| lanes[i * 2] as u32 + lanes[i * 2 + 1] as u32))
    }

    #[doc(alias = "i16x8.q15mulr_sat_s")]
    pub(crate) fn i16x8_q15mulr_sat_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        let mut out = [0i16; 8];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            let r = ((lhs as i32 * rhs as i32) + (1 << 14)) >> 15; // 2^14: Q15 rounding
            *dst = r.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
        Self::from_i16x8(out)
    }

    #[doc(alias = "i32x4.dot_i16x8_s")]
    pub(crate) fn i32x4_dot_i16x8_s(self, rhs: Self) -> Self {
        let a = self.as_i16x8();
        let b = rhs.as_i16x8();
        Self::from_i32x4(array::from_fn(|i| {
            let base = i * 2;
            (a[base] as i32)
                .wrapping_mul(b[base] as i32)
                .wrapping_add((a[base + 1] as i32).wrapping_mul(b[base + 1] as i32))
        }))
    }

    #[doc(alias = "i16x8.relaxed_dot_i8x16_i7x16_s")]
    pub(crate) fn i16x8_relaxed_dot_i8x16_i7x16_s(self, rhs: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        Self::from_i16x8(array::from_fn(|i| {
            let base = i * 2;
            let prod0 = (a[base] as i16) * (b[base] as i16);
            let prod1 = (a[base + 1] as i16) * (b[base + 1] as i16);
            prod0.wrapping_add(prod1)
        }))
    }

    #[doc(alias = "i32x4.relaxed_dot_i8x16_i7x16_add_s")]
    pub(crate) fn i32x4_relaxed_dot_i8x16_i7x16_add_s(self, rhs: Self, acc: Self) -> Self {
        let a = self.as_i8x16();
        let b = rhs.as_i8x16();
        let c = acc.as_i32x4();
        Self::from_i32x4(array::from_fn(|i| {
            let base = i * 4;
            let mut sum = c[i];
            for j in 0..4 {
                sum = sum.wrapping_add((a[base + j] as i32).wrapping_mul(b[base + j] as i32));
            }
            sum
        }))
    }

    #[doc(alias = "f32x4.relaxed_madd")]
    pub(crate) fn f32x4_relaxed_madd(self, b: Self, c: Self) -> Self {
        self.zip_f32x4(b, |x, y| canonicalize_simd_f32_nan(x * y))
            .zip_f32x4(c, |xy, z| canonicalize_simd_f32_nan(xy + z))
    }

    #[doc(alias = "f32x4.relaxed_nmadd")]
    pub(crate) fn f32x4_relaxed_nmadd(self, b: Self, c: Self) -> Self {
        self.zip_f32x4(b, |x, y| canonicalize_simd_f32_nan(-(x * y)))
            .zip_f32x4(c, |neg_xy, z| canonicalize_simd_f32_nan(neg_xy + z))
    }

    #[doc(alias = "f64x2.relaxed_madd")]
    pub(crate) fn f64x2_relaxed_madd(self, b: Self, c: Self) -> Self {
        self.zip_f64x2(b, |x, y| canonicalize_simd_f64_nan(x * y))
            .zip_f64x2(c, |xy, z| canonicalize_simd_f64_nan(xy + z))
    }

    #[doc(alias = "f64x2.relaxed_nmadd")]
    pub(crate) fn f64x2_relaxed_nmadd(self, b: Self, c: Self) -> Self {
        self.zip_f64x2(b, |x, y| canonicalize_simd_f64_nan(-(x * y)))
            .zip_f64x2(c, |neg_xy, z| canonicalize_simd_f64_nan(neg_xy + z))
    }

    #[doc(alias = "i32x4.trunc_sat_f64x2_s_zero")]
    pub(crate) fn i32x4_trunc_sat_f64x2_s_zero(self) -> Self {
        let v = self.as_f64x2();
        Self::from_i32x4([trunc_sat_f64_to_i32(v[0]), trunc_sat_f64_to_i32(v[1]), 0, 0])
    }

    #[doc(alias = "i32x4.trunc_sat_f64x2_u_zero")]
    pub(crate) fn i32x4_trunc_sat_f64x2_u_zero(self) -> Self {
        let v = self.as_f64x2();
        Self::from_u32x4([trunc_sat_f64_to_u32(v[0]), trunc_sat_f64_to_u32(v[1]), 0, 0])
    }

    #[doc(alias = "f64x2.convert_low_i32x4_s")]
    pub(crate) fn f64x2_convert_low_i32x4_s(self) -> Self {
        let [a, b, ..] = self.as_i32x4();
        Self::from_f64x2([a as f64, b as f64])
    }

    #[doc(alias = "f64x2.convert_low_i32x4_u")]
    pub(crate) fn f64x2_convert_low_i32x4_u(self) -> Self {
        let [a, b, ..] = self.as_u32x4();
        Self::from_f64x2([a as f64, b as f64])
    }

    #[doc(alias = "f32x4.demote_f64x2_zero")]
    pub(crate) fn f32x4_demote_f64x2_zero(self) -> Self {
        let [a, b] = self.as_f64x2();
        Self::from_f32x4([a as f32, b as f32, 0.0, 0.0])
    }

    #[doc(alias = "f64x2.promote_low_f32x4")]
    pub(crate) fn f64x2_promote_low_f32x4(self) -> Self {
        let [a, b, ..] = self.as_f32x4();
        Self::from_f64x2([a as f64, b as f64])
    }

    #[doc(alias = "i8x16.extract_lane_s")]
    pub(crate) fn extract_lane_i8(self, lane: u8) -> i8 {
        debug_assert!(lane < 16);
        self.0[lane as usize] as i8
    }

    #[doc(alias = "i8x16.extract_lane_u")]
    pub(crate) fn extract_lane_u8(self, lane: u8) -> u8 {
        debug_assert!(lane < 16);
        self.0[lane as usize]
    }

    impl_simd_methods! {
        // Lane splats, replacements, and extraction
        "i8x16.splat" => splat_i8(src: i8) => Self([src as u8; 16]);
        "i8x16.replace_lane" => i8x16_replace_lane(self, lane: u8, value: i8) => self.replace_lane_bytes::<1, 16>(lane, [value as u8]);
        "i16x8.replace_lane" => i16x8_replace_lane(self, lane: u8, value: i16) => self.replace_lane_bytes::<2, 8>(lane, value.to_le_bytes());
        "i32x4.replace_lane" => i32x4_replace_lane(self, lane: u8, value: i32) => self.replace_lane_bytes::<4, 4>(lane, value.to_le_bytes());
        "i64x2.replace_lane" => i64x2_replace_lane(self, lane: u8, value: i64) => self.replace_lane_bytes::<8, 2>(lane, value.to_le_bytes());
        "f32x4.replace_lane" => f32x4_replace_lane(self, lane: u8, value: f32) => self.replace_lane_bytes::<4, 4>(lane, value.to_bits().to_le_bytes());
        "f64x2.replace_lane" => f64x2_replace_lane(self, lane: u8, value: f64) => self.replace_lane_bytes::<8, 2>(lane, value.to_bits().to_le_bytes());
        "i16x8.splat" => splat_i16(src: i16) => Self::from_i16x8([src; 8]);
        "i32x4.splat" => splat_i32(src: i32) => Self::from_i32x4([src; 4]);
        "i64x2.splat" => splat_i64(src: i64) => Self::from_i64x2([src; 2]);
        "f32x4.splat" => splat_f32(src: f32) => Self::splat_i32(src.to_bits() as i32);
        "f64x2.splat" => splat_f64(src: f64) => Self::splat_i64(src.to_bits() as i64);
        "i16x8.extract_lane_s" => extract_lane_i16(self, lane: u8) -> i16 => i16::from_le_bytes(self.extract_lane_bytes::<2, 8>(lane));
        "i16x8.extract_lane_u" => extract_lane_u16(self, lane: u8) -> u16 => u16::from_le_bytes(self.extract_lane_bytes::<2, 8>(lane));
        "i32x4.extract_lane" => extract_lane_i32(self, lane: u8) -> i32 => i32::from_le_bytes(self.extract_lane_bytes::<4, 4>(lane));
        "i64x2.extract_lane" => extract_lane_i64(self, lane: u8) -> i64 => i64::from_le_bytes(self.extract_lane_bytes::<8, 2>(lane));
        "f32x4.extract_lane" => extract_lane_f32(self, lane: u8) -> f32 => f32::from_bits(self.extract_lane_i32(lane) as u32);
        "f64x2.extract_lane" => extract_lane_f64(self, lane: u8) -> f64 => f64::from_bits(self.extract_lane_i64(lane) as u64);

        // Truth checks, popcount, and averaging
        "i8x16.all_true" => i8x16_all_true(self) -> bool => self.0.iter().all(|&b| b != 0);
        "i16x8.all_true" => i16x8_all_true(self) -> bool => self.as_i16x8().iter().all(|&x| x != 0);
        "i32x4.all_true" => i32x4_all_true(self) -> bool => self.as_i32x4().iter().all(|&x| x != 0);
        "i64x2.all_true" => i64x2_all_true(self) -> bool => self.as_i64x2().iter().all(|&x| x != 0);
        "i8x16.popcnt" => i8x16_popcnt(self) => Self(self.0.map(|lane| lane.count_ones() as u8));
        "i8x16.avgr_u" => i8x16_avgr_u(self, rhs: Self) => simd_avgr_u!(self, rhs, u8x16_avgr, u8, u16, as_u8x16, from_u8x16);
        "i16x8.avgr_u" => i16x8_avgr_u(self, rhs: Self) => simd_avgr_u!(self, rhs, u16x8_avgr, u16, u32, as_u16x8, from_u16x8);

        // Relaxed aliases
        "i16x8.relaxed_q15mulr_s" => i16x8_relaxed_q15mulr_s(self, rhs: Self) => self.i16x8_q15mulr_sat_s(rhs);
        "f32x4.relaxed_min" => f32x4_relaxed_min(self, rhs: Self) => self.f32x4_min(rhs);
        "f64x2.relaxed_min" => f64x2_relaxed_min(self, rhs: Self) => self.f64x2_min(rhs);
        "f32x4.relaxed_max" => f32x4_relaxed_max(self, rhs: Self) => self.f32x4_max(rhs);
        "f64x2.relaxed_max" => f64x2_relaxed_max(self, rhs: Self) => self.f64x2_max(rhs);
        "i32x4.relaxed_trunc_f32x4_s" => i32x4_relaxed_trunc_f32x4_s(self) => self.i32x4_trunc_sat_f32x4_s();
        "i32x4.relaxed_trunc_f32x4_u" => i32x4_relaxed_trunc_f32x4_u(self) => self.i32x4_trunc_sat_f32x4_u();
        "i32x4.relaxed_trunc_f64x2_s_zero" => i32x4_relaxed_trunc_f64x2_s_zero(self) => self.i32x4_trunc_sat_f64x2_s_zero();
        "i32x4.relaxed_trunc_f64x2_u_zero" => i32x4_relaxed_trunc_f64x2_u_zero(self) => self.i32x4_trunc_sat_f64x2_u_zero();

        // Numeric conversions
        "i32x4.trunc_sat_f32x4_s" => i32x4_trunc_sat_f32x4_s(self) => Self::from_i32x4(self.as_f32x4().map(trunc_sat_f32_to_i32));
        "i32x4.trunc_sat_f32x4_u" => i32x4_trunc_sat_f32x4_u(self) => Self::from_u32x4(self.as_f32x4().map(trunc_sat_f32_to_u32));
        "f32x4.convert_i32x4_s" => f32x4_convert_i32x4_s(self) => Self::from_f32x4(self.as_i32x4().map(|x| x as f32));
        "f32x4.convert_i32x4_u" => f32x4_convert_i32x4_u(self) => Self::from_f32x4(self.as_u32x4().map(|x| x as f32));

        // Integer shifts
        "i8x16.shl" => i8x16_shl(self, shift: u32) => simd_shift!(self, shift, as_i8x16, from_i8x16, 7, shl);
        "i16x8.shl" => i16x8_shl(self, shift: u32) => simd_shift!(self, shift, as_i16x8, from_i16x8, 15, shl);
        "i32x4.shl" => i32x4_shl(self, shift: u32) => simd_shift!(self, shift, as_i32x4, from_i32x4, 31, shl);
        "i64x2.shl" => i64x2_shl(self, shift: u32) => simd_shift!(self, shift, as_i64x2, from_i64x2, 63, shl);
        "i8x16.shr_s" => i8x16_shr_s(self, shift: u32) => simd_shift!(self, shift, as_i8x16, from_i8x16, 7, shr);
        "i16x8.shr_s" => i16x8_shr_s(self, shift: u32) => simd_shift!(self, shift, as_i16x8, from_i16x8, 15, shr);
        "i32x4.shr_s" => i32x4_shr_s(self, shift: u32) => simd_shift!(self, shift, as_i32x4, from_i32x4, 31, shr);
        "i64x2.shr_s" => i64x2_shr_s(self, shift: u32) => simd_shift!(self, shift, as_i64x2, from_i64x2, 63, shr);
        "i8x16.shr_u" => i8x16_shr_u(self, shift: u32) => simd_shift!(self, shift, as_u8x16, from_u8x16, 7, shr);
        "i16x8.shr_u" => i16x8_shr_u(self, shift: u32) => simd_shift!(self, shift, as_u16x8, from_u16x8, 15, shr);
        "i32x4.shr_u" => i32x4_shr_u(self, shift: u32) => simd_shift!(self, shift, as_u32x4, from_u32x4, 31, shr);
        "i64x2.shr_u" => i64x2_shr_u(self, shift: u32) => simd_shift!(self, shift, as_u64x2, from_u64x2, 63, shr);

        // Integer arithmetic
        "i8x16.add" => i8x16_add(self, rhs: Self) => simd_binop!(self, rhs, i8x16_add, as_i8x16, from_i8x16, wrapping_add);
        "i16x8.add" => i16x8_add(self, rhs: Self) => simd_binop!(self, rhs, i16x8_add, as_i16x8, from_i16x8, wrapping_add);
        "i32x4.add" => i32x4_add(self, rhs: Self) => simd_binop!(self, rhs, i32x4_add, as_i32x4, from_i32x4, wrapping_add);
        "i64x2.add" => i64x2_add(self, rhs: Self) => simd_binop!(self, rhs, i64x2_add, as_i64x2, from_i64x2, wrapping_add);
        "i8x16.sub" => i8x16_sub(self, rhs: Self) => simd_binop!(self, rhs, i8x16_sub, as_i8x16, from_i8x16, wrapping_sub);
        "i16x8.sub" => i16x8_sub(self, rhs: Self) => simd_binop!(self, rhs, i16x8_sub, as_i16x8, from_i16x8, wrapping_sub);
        "i32x4.sub" => i32x4_sub(self, rhs: Self) => simd_binop!(self, rhs, i32x4_sub, as_i32x4, from_i32x4, wrapping_sub);
        "i64x2.sub" => i64x2_sub(self, rhs: Self) => simd_binop!(self, rhs, i64x2_sub, as_i64x2, from_i64x2, wrapping_sub);
        "i16x8.mul" => i16x8_mul(self, rhs: Self) => simd_binop!(self, rhs, i16x8_mul, as_i16x8, from_i16x8, wrapping_mul);
        "i32x4.mul" => i32x4_mul(self, rhs: Self) => simd_binop!(self, rhs, i32x4_mul, as_i32x4, from_i32x4, wrapping_mul);
        "i64x2.mul" => i64x2_mul(self, rhs: Self) => simd_binop!(self, rhs, i64x2_mul, as_i64x2, from_i64x2, wrapping_mul);
        "i8x16.add_sat_s" => i8x16_add_sat_s(self, rhs: Self) => simd_binop!(self, rhs, i8x16_add_sat, as_i8x16, from_i8x16, saturating_add);
        "i16x8.add_sat_s" => i16x8_add_sat_s(self, rhs: Self) => simd_binop!(self, rhs, i16x8_add_sat, as_i16x8, from_i16x8, saturating_add);
        "i8x16.add_sat_u" => i8x16_add_sat_u(self, rhs: Self) => simd_binop!(self, rhs, u8x16_add_sat, as_u8x16, from_u8x16, saturating_add);
        "i16x8.add_sat_u" => i16x8_add_sat_u(self, rhs: Self) => simd_binop!(self, rhs, u16x8_add_sat, as_u16x8, from_u16x8, saturating_add);
        "i8x16.sub_sat_s" => i8x16_sub_sat_s(self, rhs: Self) => simd_binop!(self, rhs, i8x16_sub_sat, as_i8x16, from_i8x16, saturating_sub);
        "i16x8.sub_sat_s" => i16x8_sub_sat_s(self, rhs: Self) => simd_binop!(self, rhs, i16x8_sub_sat, as_i16x8, from_i16x8, saturating_sub);
        "i8x16.sub_sat_u" => i8x16_sub_sat_u(self, rhs: Self) => simd_binop!(self, rhs, u8x16_sub_sat, as_u8x16, from_u8x16, saturating_sub);
        "i16x8.sub_sat_u" => i16x8_sub_sat_u(self, rhs: Self) => simd_binop!(self, rhs, u16x8_sub_sat, as_u16x8, from_u16x8, saturating_sub);

        // Integer lane extension
        "i16x8.extend_low_i8x16_s" => i16x8_extend_low_i8x16_s(self) => simd_extend_cast!(self, as_i8x16, from_i16x8, i16, 0);
        "i16x8.extend_low_i8x16_u" => i16x8_extend_low_i8x16_u(self) => simd_extend_cast!(self, as_u8x16, from_u16x8, u16, 0);
        "i16x8.extend_high_i8x16_s" => i16x8_extend_high_i8x16_s(self) => simd_extend_cast!(self, as_i8x16, from_i16x8, i16, 8);
        "i16x8.extend_high_i8x16_u" => i16x8_extend_high_i8x16_u(self) => simd_extend_cast!(self, as_u8x16, from_u16x8, u16, 8);
        "i32x4.extend_low_i16x8_s" => i32x4_extend_low_i16x8_s(self) => simd_extend_cast!(self, as_i16x8, from_i32x4, i32, 0);
        "i32x4.extend_low_i16x8_u" => i32x4_extend_low_i16x8_u(self) => simd_extend_cast!(self, as_u16x8, from_u32x4, u32, 0);
        "i32x4.extend_high_i16x8_s" => i32x4_extend_high_i16x8_s(self) => simd_extend_cast!(self, as_i16x8, from_i32x4, i32, 4);
        "i32x4.extend_high_i16x8_u" => i32x4_extend_high_i16x8_u(self) => simd_extend_cast!(self, as_u16x8, from_u32x4, u32, 4);
        "i64x2.extend_low_i32x4_s" => i64x2_extend_low_i32x4_s(self) => simd_extend_cast!(self, as_i32x4, from_i64x2, i64, 0);
        "i64x2.extend_low_i32x4_u" => i64x2_extend_low_i32x4_u(self) => simd_extend_cast!(self, as_u32x4, from_u64x2, u64, 0);
        "i64x2.extend_high_i32x4_s" => i64x2_extend_high_i32x4_s(self) => simd_extend_cast!(self, as_i32x4, from_i64x2, i64, 2);
        "i64x2.extend_high_i32x4_u" => i64x2_extend_high_i32x4_u(self) => simd_extend_cast!(self, as_u32x4, from_u64x2, u64, 2);

        // Extended multiplication
        "i16x8.extmul_low_i8x16_s" => i16x8_extmul_low_i8x16_s(self, rhs: Self) => simd_extmul!(self, rhs, as_i8x16, from_i16x8, i16, 0);
        "i16x8.extmul_low_i8x16_u" => i16x8_extmul_low_i8x16_u(self, rhs: Self) => simd_extmul!(self, rhs, as_u8x16, from_u16x8, u16, 0);
        "i16x8.extmul_high_i8x16_s" => i16x8_extmul_high_i8x16_s(self, rhs: Self) => simd_extmul!(self, rhs, as_i8x16, from_i16x8, i16, 8);
        "i16x8.extmul_high_i8x16_u" => i16x8_extmul_high_i8x16_u(self, rhs: Self) => simd_extmul!(self, rhs, as_u8x16, from_u16x8, u16, 8);
        "i32x4.extmul_low_i16x8_s" => i32x4_extmul_low_i16x8_s(self, rhs: Self) => simd_extmul!(self, rhs, as_i16x8, from_i32x4, i32, 0);
        "i32x4.extmul_low_i16x8_u" => i32x4_extmul_low_i16x8_u(self, rhs: Self) => simd_extmul!(self, rhs, as_u16x8, from_u32x4, u32, 0);
        "i32x4.extmul_high_i16x8_s" => i32x4_extmul_high_i16x8_s(self, rhs: Self) => simd_extmul!(self, rhs, as_i16x8, from_i32x4, i32, 4);
        "i32x4.extmul_high_i16x8_u" => i32x4_extmul_high_i16x8_u(self, rhs: Self) => simd_extmul!(self, rhs, as_u16x8, from_u32x4, u32, 4);
        "i64x2.extmul_low_i32x4_s" => i64x2_extmul_low_i32x4_s(self, rhs: Self) => simd_extmul!(self, rhs, as_i32x4, from_i64x2, i64, 0);
        "i64x2.extmul_low_i32x4_u" => i64x2_extmul_low_i32x4_u(self, rhs: Self) => simd_extmul!(self, rhs, as_u32x4, from_u64x2, u64, 0);
        "i64x2.extmul_high_i32x4_s" => i64x2_extmul_high_i32x4_s(self, rhs: Self) => simd_extmul!(self, rhs, as_i32x4, from_i64x2, i64, 2);
        "i64x2.extmul_high_i32x4_u" => i64x2_extmul_high_i32x4_u(self, rhs: Self) => simd_extmul!(self, rhs, as_u32x4, from_u64x2, u64, 2);

        // Relaxed lane selection
        "i8x16.relaxed_laneselect" => i8x16_relaxed_laneselect(v1: Self, v2: Self, c: Self) => Self::v128_bitselect(v1, v2, c);
        "i16x8.relaxed_laneselect" => i16x8_relaxed_laneselect(v1: Self, v2: Self, c: Self) => Self::v128_bitselect(v1, v2, c);
        "i32x4.relaxed_laneselect" => i32x4_relaxed_laneselect(v1: Self, v2: Self, c: Self) => Self::v128_bitselect(v1, v2, c);
        "i64x2.relaxed_laneselect" => i64x2_relaxed_laneselect(v1: Self, v2: Self, c: Self) => Self::v128_bitselect(v1, v2, c);

        // Integer comparisons
        "i8x16.eq" => i8x16_eq(self, rhs: Self) => simd_cmp_mask!(self, rhs, i8x16_eq, as_i8x16, from_i8x16, ==);
        "i16x8.eq" => i16x8_eq(self, rhs: Self) => simd_cmp_mask!(self, rhs, i16x8_eq, as_i16x8, from_i16x8, ==);
        "i32x4.eq" => i32x4_eq(self, rhs: Self) => simd_cmp_mask!(self, rhs, i32x4_eq, as_i32x4, from_i32x4, ==);
        "i64x2.eq" => i64x2_eq(self, rhs: Self) => simd_cmp_mask!(self, rhs, i64x2_eq, as_i64x2, from_i64x2, ==);
        "i8x16.ne" => i8x16_ne(self, rhs: Self) => simd_cmp_mask!(self, rhs, i8x16_ne, as_i8x16, from_i8x16, !=);
        "i16x8.ne" => i16x8_ne(self, rhs: Self) => simd_cmp_mask!(self, rhs, i16x8_ne, as_i16x8, from_i16x8, !=);
        "i32x4.ne" => i32x4_ne(self, rhs: Self) => simd_cmp_mask!(self, rhs, i32x4_ne, as_i32x4, from_i32x4, !=);
        "i64x2.ne" => i64x2_ne(self, rhs: Self) => simd_cmp_mask!(self, rhs, i64x2_ne, as_i64x2, from_i64x2, !=);
        "i8x16.lt_s" => i8x16_lt_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i8x16_lt, as_i8x16, from_i8x16, <);
        "i16x8.lt_s" => i16x8_lt_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i16x8_lt, as_i16x8, from_i16x8, <);
        "i32x4.lt_s" => i32x4_lt_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i32x4_lt, as_i32x4, from_i32x4, <);
        "i64x2.lt_s" => i64x2_lt_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i64x2_lt, as_i64x2, from_i64x2, <);
        "i8x16.lt_u" => i8x16_lt_u(self, rhs: Self) => simd_cmp_mask!(self, rhs, u8x16_lt, as_u8x16, from_i8x16, <);
        "i16x8.lt_u" => i16x8_lt_u(self, rhs: Self) => simd_cmp_mask!(self, rhs, u16x8_lt, as_u16x8, from_i16x8, <);
        "i32x4.lt_u" => i32x4_lt_u(self, rhs: Self) => simd_cmp_mask!(self, rhs, u32x4_lt, as_u32x4, from_i32x4, <);
        "i8x16.ge_s" => i8x16_ge_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i8x16_ge, as_i8x16, from_i8x16, >=);
        "i16x8.ge_s" => i16x8_ge_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i16x8_ge, as_i16x8, from_i16x8, >=);
        "i32x4.ge_s" => i32x4_ge_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i32x4_ge, as_i32x4, from_i32x4, >=);
        "i64x2.ge_s" => i64x2_ge_s(self, rhs: Self) => simd_cmp_mask!(self, rhs, i64x2_ge, as_i64x2, from_i64x2, >=);
        "i8x16.ge_u" => i8x16_ge_u(self, rhs: Self) => simd_cmp_mask!(self, rhs, u8x16_ge, as_u8x16, from_i8x16, >=);
        "i16x8.ge_u" => i16x8_ge_u(self, rhs: Self) => simd_cmp_mask!(self, rhs, u16x8_ge, as_u16x8, from_i16x8, >=);
        "i32x4.ge_u" => i32x4_ge_u(self, rhs: Self) => simd_cmp_mask!(self, rhs, u32x4_ge, as_u32x4, from_i32x4, >=);

        // Reversed comparisons
        "i8x16.gt_s" => i8x16_gt_s(self, rhs: Self) => rhs.i8x16_lt_s(self);
        "i16x8.gt_s" => i16x8_gt_s(self, rhs: Self) => rhs.i16x8_lt_s(self);
        "i32x4.gt_s" => i32x4_gt_s(self, rhs: Self) => rhs.i32x4_lt_s(self);
        "i64x2.gt_s" => i64x2_gt_s(self, rhs: Self) => rhs.i64x2_lt_s(self);
        "i8x16.gt_u" => i8x16_gt_u(self, rhs: Self) => rhs.i8x16_lt_u(self);
        "i16x8.gt_u" => i16x8_gt_u(self, rhs: Self) => rhs.i16x8_lt_u(self);
        "i32x4.gt_u" => i32x4_gt_u(self, rhs: Self) => rhs.i32x4_lt_u(self);
        "i8x16.le_s" => i8x16_le_s(self, rhs: Self) => rhs.i8x16_ge_s(self);
        "i16x8.le_s" => i16x8_le_s(self, rhs: Self) => rhs.i16x8_ge_s(self);
        "i32x4.le_s" => i32x4_le_s(self, rhs: Self) => rhs.i32x4_ge_s(self);
        "i64x2.le_s" => i64x2_le_s(self, rhs: Self) => rhs.i64x2_ge_s(self);
        "i8x16.le_u" => i8x16_le_u(self, rhs: Self) => rhs.i8x16_ge_u(self);
        "i16x8.le_u" => i16x8_le_u(self, rhs: Self) => rhs.i16x8_ge_u(self);
        "i32x4.le_u" => i32x4_le_u(self, rhs: Self) => rhs.i32x4_ge_u(self);

        // Integer unary operations
        "i8x16.abs" => i8x16_abs(self) => simd_abs_const!(self, as_i8x16, from_i8x16);
        "i16x8.abs" => i16x8_abs(self) => simd_abs_const!(self, as_i16x8, from_i16x8);
        "i32x4.abs" => i32x4_abs(self) => simd_abs_const!(self, as_i32x4, from_i32x4);
        "i64x2.abs" => i64x2_abs(self) => simd_abs_const!(self, as_i64x2, from_i64x2);
        "i8x16.neg" => i8x16_neg(self) => simd_neg!(self, i8x16_neg, as_i8x16, from_i8x16);
        "i16x8.neg" => i16x8_neg(self) => simd_neg!(self, i16x8_neg, as_i16x8, from_i16x8);
        "i32x4.neg" => i32x4_neg(self) => simd_neg!(self, i32x4_neg, as_i32x4, from_i32x4);
        "i64x2.neg" => i64x2_neg(self) => simd_neg!(self, i64x2_neg, as_i64x2, from_i64x2);

        // Integer min and max
        "i8x16.min_s" => i8x16_min_s(self, rhs: Self) => simd_minmax!(self, rhs, i8x16_min, as_i8x16, from_i8x16, <);
        "i16x8.min_s" => i16x8_min_s(self, rhs: Self) => simd_minmax!(self, rhs, i16x8_min, as_i16x8, from_i16x8, <);
        "i32x4.min_s" => i32x4_min_s(self, rhs: Self) => simd_minmax!(self, rhs, i32x4_min, as_i32x4, from_i32x4, <);
        "i8x16.min_u" => i8x16_min_u(self, rhs: Self) => simd_minmax!(self, rhs, u8x16_min, as_u8x16, from_u8x16, <);
        "i16x8.min_u" => i16x8_min_u(self, rhs: Self) => simd_minmax!(self, rhs, u16x8_min, as_u16x8, from_u16x8, <);
        "i32x4.min_u" => i32x4_min_u(self, rhs: Self) => simd_minmax!(self, rhs, u32x4_min, as_u32x4, from_u32x4, <);
        "i8x16.max_s" => i8x16_max_s(self, rhs: Self) => simd_minmax!(self, rhs, i8x16_max, as_i8x16, from_i8x16, >);
        "i16x8.max_s" => i16x8_max_s(self, rhs: Self) => simd_minmax!(self, rhs, i16x8_max, as_i16x8, from_i16x8, >);
        "i32x4.max_s" => i32x4_max_s(self, rhs: Self) => simd_minmax!(self, rhs, i32x4_max, as_i32x4, from_i32x4, >);
        "i8x16.max_u" => i8x16_max_u(self, rhs: Self) => simd_minmax!(self, rhs, u8x16_max, as_u8x16, from_u8x16, >);
        "i16x8.max_u" => i16x8_max_u(self, rhs: Self) => simd_minmax!(self, rhs, u16x8_max, as_u16x8, from_u16x8, >);
        "i32x4.max_u" => i32x4_max_u(self, rhs: Self) => simd_minmax!(self, rhs, u32x4_max, as_u32x4, from_u32x4, >);

        // Float comparisons
        "f32x4.eq" => f32x4_eq(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f32x4, from_i32x4, ==);
        "f64x2.eq" => f64x2_eq(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f64x2, from_i64x2, ==);
        "f32x4.ne" => f32x4_ne(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f32x4, from_i32x4, !=);
        "f64x2.ne" => f64x2_ne(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f64x2, from_i64x2, !=);
        "f32x4.lt" => f32x4_lt(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f32x4, from_i32x4, <);
        "f64x2.lt" => f64x2_lt(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f64x2, from_i64x2, <);
        "f32x4.gt" => f32x4_gt(self, rhs: Self) => rhs.f32x4_lt(self);
        "f64x2.gt" => f64x2_gt(self, rhs: Self) => rhs.f64x2_lt(self);
        "f32x4.le" => f32x4_le(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f32x4, from_i32x4, <=);
        "f64x2.le" => f64x2_le(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f64x2, from_i64x2, <=);
        "f32x4.ge" => f32x4_ge(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f32x4, from_i32x4, >=);
        "f64x2.ge" => f64x2_ge(self, rhs: Self) => simd_cmp_mask!(generic self, rhs, as_f64x2, from_i64x2, >=);

        // Float arithmetic
        "f32x4.ceil" => f32x4_ceil(self) => self.map_f32x4(|x| canonicalize_simd_f32_nan(x.ceil()));
        "f64x2.ceil" => f64x2_ceil(self) => self.map_f64x2(|x| canonicalize_simd_f64_nan(x.ceil()));
        "f32x4.floor" => f32x4_floor(self) => self.map_f32x4(|x| canonicalize_simd_f32_nan(x.floor()));
        "f64x2.floor" => f64x2_floor(self) => self.map_f64x2(|x| canonicalize_simd_f64_nan(x.floor()));
        "f32x4.trunc" => f32x4_trunc(self) => self.map_f32x4(|x| canonicalize_simd_f32_nan(x.trunc()));
        "f64x2.trunc" => f64x2_trunc(self) => self.map_f64x2(|x| canonicalize_simd_f64_nan(x.trunc()));
        "f32x4.nearest" => f32x4_nearest(self) => self.map_f32x4(|x| canonicalize_simd_f32_nan(TinywasmFloatExt::tw_nearest(x)));
        "f64x2.nearest" => f64x2_nearest(self) => self.map_f64x2(|x| canonicalize_simd_f64_nan(TinywasmFloatExt::tw_nearest(x)));
        "f32x4.abs" => f32x4_abs(self) => self.map_f32x4(f32::abs);
        "f64x2.abs" => f64x2_abs(self) => self.map_f64x2(f64::abs);
        "f32x4.neg" => f32x4_neg(self) => self.map_f32x4(|x| -x);
        "f64x2.neg" => f64x2_neg(self) => self.map_f64x2(|x| -x);
        "f32x4.sqrt" => f32x4_sqrt(self) => self.map_f32x4(|x| canonicalize_simd_f32_nan(x.sqrt()));
        "f64x2.sqrt" => f64x2_sqrt(self) => self.map_f64x2(|x| canonicalize_simd_f64_nan(x.sqrt()));
        "f32x4.add" => f32x4_add(self, rhs: Self) => self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a + b));
        "f64x2.add" => f64x2_add(self, rhs: Self) => self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a + b));
        "f32x4.sub" => f32x4_sub(self, rhs: Self) => self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a - b));
        "f64x2.sub" => f64x2_sub(self, rhs: Self) => self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a - b));
        "f32x4.mul" => f32x4_mul(self, rhs: Self) => self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a * b));
        "f64x2.mul" => f64x2_mul(self, rhs: Self) => self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a * b));
        "f32x4.div" => f32x4_div(self, rhs: Self) => self.zip_f32x4(rhs, |a, b| canonicalize_simd_f32_nan(a / b));
        "f64x2.div" => f64x2_div(self, rhs: Self) => self.zip_f64x2(rhs, |a, b| canonicalize_simd_f64_nan(a / b));
        "f32x4.min" => f32x4_min(self, rhs: Self) => self.zip_f32x4(rhs, TinywasmFloatExt::tw_minimum);
        "f64x2.min" => f64x2_min(self, rhs: Self) => self.zip_f64x2(rhs, TinywasmFloatExt::tw_minimum);
        "f32x4.max" => f32x4_max(self, rhs: Self) => self.zip_f32x4(rhs, TinywasmFloatExt::tw_maximum);
        "f64x2.max" => f64x2_max(self, rhs: Self) => self.zip_f64x2(rhs, TinywasmFloatExt::tw_maximum);
        "f32x4.pmin" => f32x4_pmin(self, rhs: Self) => self.zip_f32x4(rhs, |a, b| if b < a { b } else { a });
        "f64x2.pmin" => f64x2_pmin(self, rhs: Self) => self.zip_f64x2(rhs, |a, b| if b < a { b } else { a });
        "f32x4.pmax" => f32x4_pmax(self, rhs: Self) => self.zip_f32x4(rhs, |a, b| if b > a { b } else { a });
        "f64x2.pmax" => f64x2_pmax(self, rhs: Self) => self.zip_f64x2(rhs, |a, b| if b > a { b } else { a });
    }
}
