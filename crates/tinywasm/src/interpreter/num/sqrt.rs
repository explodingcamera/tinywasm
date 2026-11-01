// SPDX-License-Identifier: MIT
// Square root is based on musl's sqrt.c, sqrtf.c, and sqrt_data.c.
// Copyright © 2005-2020 Rich Felker, et al.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

#[inline]
fn mul_high32(a: u32, b: u32) -> u32 {
    ((u64::from(a) * u64::from(b)) >> 32) as u32
}

#[inline]
fn mul_high64(a: u64, b: u64) -> u64 {
    ((u128::from(a) * u128::from(b)) >> 64) as u64
}

/// Computes a correctly rounded software square root using fixed-point refinement.
#[inline]
pub(crate) fn sqrt_f32(x: f32) -> f32 {
    let mut bits = x.to_bits();
    if bits.wrapping_sub(0x0080_0000) >= 0x7f00_0000 {
        if bits.wrapping_mul(2) == 0 || bits == 0x7f80_0000 {
            return x;
        }
        if bits > 0x7f80_0000 {
            return if x.is_nan() { x + x } else { f32::NAN };
        }
        bits = (x * 8_388_608.0).to_bits().wrapping_sub(23 << 23);
    }
    // Reduce x to 4^e * m, with m in [1, 4) represented as unsigned 2.30 fixed point.
    let m = if bits & 0x0080_0000 != 0 { (bits << 7) & 0x7fff_ffff } else { (bits << 8) | 0x8000_0000 };
    let exponent = (bits >> 1).wrapping_add(0x1fc0_0000) & 0x7f80_0000;
    let mut r = u32::from(RSQRT_TABLE[((bits >> 17) & 127) as usize]) << 16;
    let mut s = mul_high32(m, r);
    let u = 0xc000_0000u32.wrapping_sub(mul_high32(s, r));
    r = mul_high32(r, u) << 1;
    s = mul_high32(s, u) << 1;
    let u = 0xc000_0000u32.wrapping_sub(mul_high32(s, r));
    s = mul_high32(s, u).wrapping_sub(1) >> 6;
    // The approximation is below sqrt(m). The residual selects the nearest result.
    let residual = (m << 16).wrapping_sub(s.wrapping_mul(s));
    s += s.wrapping_sub(residual) >> 31;
    f32::from_bits((s & 0x007f_ffff) | exponent)
}

/// Computes a correctly rounded software square root using fixed-point refinement.
#[inline]
pub(crate) fn sqrt_f64(x: f64) -> f64 {
    let mut bits = x.to_bits();
    let mut top = (bits >> 52) as u32;
    if top.wrapping_sub(1) >= 0x7fe {
        if bits.wrapping_mul(2) == 0 || bits == 0x7ff0_0000_0000_0000 {
            return x;
        }
        if bits > 0x7ff0_0000_0000_0000 {
            return if x.is_nan() { x + x } else { f64::NAN };
        }
        bits = (x * 4_503_599_627_370_496.0).to_bits();
        top = ((bits >> 52) as u32).wrapping_sub(52);
    }
    // Reduce x to 4^e * m, with m in [1, 4) represented as unsigned 2.62 fixed point.
    let mut m = (bits << 11) | 0x8000_0000_0000_0000;
    if top & 1 != 0 {
        m >>= 1;
    }
    let exponent = u64::from(top.wrapping_add(0x3ff) >> 1);
    let mut r = u32::from(RSQRT_TABLE[((bits >> 46) & 127) as usize]) << 16;
    let mut s = mul_high32((m >> 32) as u32, r);
    let u = 0xc000_0000u32.wrapping_sub(mul_high32(s, r));
    r = mul_high32(r, u) << 1;
    s = mul_high32(s, u) << 1;
    let u = 0xc000_0000u32.wrapping_sub(mul_high32(s, r));
    let r = u64::from(mul_high32(r, u) << 1) << 32;
    let s = mul_high64(m, r);
    let u = 0xc000_0000_0000_0000u64.wrapping_sub(mul_high64(s, r));
    let mut s = mul_high64(s, u) >> 9;
    let residual = (m << 42).wrapping_sub(s.wrapping_mul(s));
    s += s.wrapping_sub(residual) >> 63;
    f64::from_bits((s & 0x000f_ffff_ffff_ffff) | (exponent << 52))
}

#[rustfmt::skip]
const RSQRT_TABLE: [u16; 128] = [
    0xb451, 0xb2f0, 0xb196, 0xb044, 0xaef9, 0xadb6, 0xac79, 0xab43,
    0xaa14, 0xa8eb, 0xa7c8, 0xa6aa, 0xa592, 0xa480, 0xa373, 0xa26b,
    0xa168, 0xa06a, 0x9f70, 0x9e7b, 0x9d8a, 0x9c9d, 0x9bb5, 0x9ad1,
    0x99f0, 0x9913, 0x983a, 0x9765, 0x9693, 0x95c4, 0x94f8, 0x9430,
    0x936b, 0x92a9, 0x91ea, 0x912e, 0x9075, 0x8fbe, 0x8f0a, 0x8e59,
    0x8daa, 0x8cfe, 0x8c54, 0x8bac, 0x8b07, 0x8a64, 0x89c4, 0x8925,
    0x8889, 0x87ee, 0x8756, 0x86c0, 0x862b, 0x8599, 0x8508, 0x8479,
    0x83ec, 0x8361, 0x82d8, 0x8250, 0x81c9, 0x8145, 0x80c2, 0x8040,
    0xff02, 0xfd0e, 0xfb25, 0xf947, 0xf773, 0xf5aa, 0xf3ea, 0xf234,
    0xf087, 0xeee3, 0xed47, 0xebb3, 0xea27, 0xe8a3, 0xe727, 0xe5b2,
    0xe443, 0xe2dc, 0xe17a, 0xe020, 0xdecb, 0xdd7d, 0xdc34, 0xdaf1,
    0xd9b3, 0xd87b, 0xd748, 0xd61a, 0xd4f1, 0xd3cd, 0xd2ad, 0xd192,
    0xd07b, 0xcf69, 0xce5b, 0xcd51, 0xcc4a, 0xcb48, 0xca4a, 0xc94f,
    0xc858, 0xc764, 0xc674, 0xc587, 0xc49d, 0xc3b7, 0xc2d4, 0xc1f4,
    0xc116, 0xc03c, 0xbf65, 0xbe90, 0xbdbe, 0xbcef, 0xbc23, 0xbb59,
    0xba91, 0xb9cc, 0xb90a, 0xb84a, 0xb78c, 0xb6d0, 0xb617, 0xb560,
];
