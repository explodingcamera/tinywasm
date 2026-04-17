#![allow(unused_macros)]

macro_rules! simd_impl {
    ($(wasm => $wasm:block)? $(x86 => $x86:block)? generic => $generic:block) => {
        cfg_select! {
            any(target_arch = "wasm32", target_arch = "wasm64") => {
                simd_impl!(@pick_wasm $( $wasm )? ; $generic)
            },

            all(
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
            ) => {
                simd_impl!(@pick_x86 $( $x86 )? ; $generic)
            },

            _ => {
                $generic
            }
        }
    };

    (@pick_wasm $wasm:block ; $generic:block) => {
        $wasm
    };

    (@pick_wasm ; $generic:block) => {
        $generic
    };

    (@pick_x86 $x86:block ; $generic:block) => {
        $x86
    };

    (@pick_x86 ; $generic:block) => {
        $generic
    };
}

macro_rules! simd_wrapping_binop_generic {
    ($lhs:expr, $rhs:expr, $lane_ty:ty, $lane_count:expr, $as_lanes:ident, $from_lanes:ident, $op:ident) => {{
        let a = $lhs.$as_lanes();
        let b = $rhs.$as_lanes();
        let mut out = [0 as $lane_ty; $lane_count];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            *dst = lhs.$op(rhs);
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_wrapping_binop {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $lane_ty:ty, $lane_count:expr, $as_lanes:ident, $from_lanes:ident, $op:ident) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => { simd_wrapping_binop_generic!($lhs, $rhs, $lane_ty, $lane_count, $as_lanes, $from_lanes, $op) }
        }
    }};
}

macro_rules! simd_sat_binop_generic {
    ($lhs:expr, $rhs:expr, $lane_ty:ty, $lane_count:expr, $as_lanes:ident, $from_lanes:ident, $op:ident) => {{
        let a = $lhs.$as_lanes();
        let b = $rhs.$as_lanes();
        let mut out = [0 as $lane_ty; $lane_count];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            *dst = lhs.$op(rhs);
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_sat_binop {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $lane_ty:ty, $lane_count:expr, $as_lanes:ident, $from_lanes:ident, $op:ident) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => { simd_sat_binop_generic!($lhs, $rhs, $lane_ty, $lane_count, $as_lanes, $from_lanes, $op) }
        }
    }};
}

macro_rules! simd_shift_left {
    ($value:expr, $shift:expr, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $mask:expr) => {{
        let lanes = $value.$as_lanes();
        let s = $shift & $mask;
        let mut out = [0 as $lane_ty; $count];
        for (dst, lane) in out.iter_mut().zip(lanes) {
            *dst = lane.wrapping_shl(s);
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_shift_right {
    ($value:expr, $shift:expr, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $mask:expr) => {{
        let lanes = $value.$as_lanes();
        let s = $shift & $mask;
        let mut out = [0 as $lane_ty; $count];
        for (dst, lane) in out.iter_mut().zip(lanes) {
            *dst = lane >> s;
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_avgr_u_generic {
    ($lhs:expr, $rhs:expr, $lane_ty:ty, $wide_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {{
        let a = $lhs.$as_lanes();
        let b = $rhs.$as_lanes();
        let mut out = [0 as $lane_ty; $count];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            *dst = ((lhs as $wide_ty + rhs as $wide_ty + 1) >> 1) as $lane_ty;
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_avgr_u {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $lane_ty:ty, $wide_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => { simd_avgr_u_generic!($lhs, $rhs, $lane_ty, $wide_ty, $count, $as_lanes, $from_lanes) }
        }
    }};
}

macro_rules! simd_extend_cast {
    ($value:expr, $src_as:ident, $dst_from:ident, $dst_ty:ty, $dst_count:expr, $offset:expr) => {{
        let lanes = $value.$src_as();
        let mut out = [0 as $dst_ty; $dst_count];
        for (dst, src) in out.iter_mut().zip(lanes[$offset..($offset + $dst_count)].iter()) {
            *dst = *src as $dst_ty;
        }
        Self::$dst_from(out)
    }};
}

macro_rules! simd_extmul_signed {
    ($lhs:expr, $rhs:expr, $src_as:ident, $dst_from:ident, $dst_ty:ty, $dst_count:expr, $offset:expr) => {{
        let a = $lhs.$src_as();
        let b = $rhs.$src_as();
        let mut out = [0 as $dst_ty; $dst_count];
        for ((dst, lhs), rhs) in
            out.iter_mut().zip(a[$offset..($offset + $dst_count)].iter()).zip(b[$offset..($offset + $dst_count)].iter())
        {
            *dst = (*lhs as $dst_ty).wrapping_mul(*rhs as $dst_ty);
        }
        Self::$dst_from(out)
    }};
}

macro_rules! simd_extmul_unsigned {
    ($lhs:expr, $rhs:expr, $src_as:ident, $dst_from:ident, $dst_ty:ty, $dst_count:expr, $offset:expr) => {{
        let a = $lhs.$src_as();
        let b = $rhs.$src_as();
        let mut out = [0 as $dst_ty; $dst_count];
        for ((dst, lhs), rhs) in
            out.iter_mut().zip(a[$offset..($offset + $dst_count)].iter()).zip(b[$offset..($offset + $dst_count)].iter())
        {
            *dst = (*lhs as $dst_ty) * (*rhs as $dst_ty);
        }
        Self::$dst_from(out)
    }};
}

macro_rules! simd_cmp_mask_generic {
    ($lhs:expr, $rhs:expr, $out_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        let a = $lhs.$as_lanes();
        let b = $rhs.$as_lanes();
        let mut out = [0 as $out_ty; $count];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            *dst = if lhs $cmp rhs { -1 } else { 0 };
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_cmp_mask {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $out_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => { simd_cmp_mask_generic!($lhs, $rhs, $out_ty, $count, $as_lanes, $from_lanes, $cmp) }
        }
    }};
}

macro_rules! simd_cmp_mask_const {
    ($lhs:expr, $rhs:expr, $out_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        let a = $lhs.$as_lanes();
        let b = $rhs.$as_lanes();
        let mut out = [0 as $out_ty; $count];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            *dst = if lhs $cmp rhs { -1 } else { 0 };
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_cmp_delegate {
    ($lhs:expr, $rhs:expr, $delegate:ident) => {{ $rhs.$delegate($lhs) }};
}

macro_rules! simd_abs_const {
    ($value:expr, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {{
        let a = $value.$as_lanes();
        let mut out = [0 as $lane_ty; $count];
        for (dst, lane) in out.iter_mut().zip(a) {
            *dst = lane.wrapping_abs();
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_neg_generic {
    ($value:expr, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {{
        let a = $value.$as_lanes();
        let mut out = [0 as $lane_ty; $count];
        for (dst, lane) in out.iter_mut().zip(a) {
            *dst = lane.wrapping_neg();
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_neg {
    ($value:expr, $wasm_op:ident, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($value.to_wasm_v128())) }
            generic => { simd_neg_generic!($value, $lane_ty, $count, $as_lanes, $from_lanes) }
        }
    }};
}

macro_rules! simd_minmax_generic {
    ($lhs:expr, $rhs:expr, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        let a = $lhs.$as_lanes();
        let b = $rhs.$as_lanes();
        let mut out = [0 as $lane_ty; $count];
        for ((dst, lhs), rhs) in out.iter_mut().zip(a).zip(b) {
            *dst = if lhs $cmp rhs { lhs } else { rhs };
        }
        Self::$from_lanes(out)
    }};
}

macro_rules! simd_minmax {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $lane_ty:ty, $count:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => { simd_minmax_generic!($lhs, $rhs, $lane_ty, $count, $as_lanes, $from_lanes, $cmp) }
        }
    }};
}

macro_rules! simd_float_unary {
    ($value:expr, $map:ident, $op:expr) => {{ $value.$map($op) }};
}

macro_rules! simd_float_binary {
    ($lhs:expr, $rhs:expr, $zip:ident, $op:expr) => {{ $lhs.$zip($rhs, $op) }};
}

#[rustfmt::skip]
macro_rules! lane_read {
    (i8,  $bytes:expr, $offset:expr) => { $bytes[$offset] as i8 };
    (u8,  $bytes:expr, $offset:expr) => { $bytes[$offset] };
    (i16, $bytes:expr, $offset:expr) => { i16::from_le_bytes([$bytes[$offset], $bytes[$offset + 1]]) };
    (u16, $bytes:expr, $offset:expr) => { u16::from_le_bytes([$bytes[$offset], $bytes[$offset + 1]]) };
    (i32, $bytes:expr, $offset:expr) => { i32::from_le_bytes([$bytes[$offset], $bytes[$offset + 1], $bytes[$offset + 2], $bytes[$offset + 3]]) };
    (u32, $bytes:expr, $offset:expr) => { u32::from_le_bytes([$bytes[$offset], $bytes[$offset + 1], $bytes[$offset + 2], $bytes[$offset + 3]]) };
    (i64, $bytes:expr, $offset:expr) => { i64::from_le_bytes([$bytes[$offset], $bytes[$offset + 1], $bytes[$offset + 2], $bytes[$offset + 3], $bytes[$offset + 4], $bytes[$offset + 5], $bytes[$offset + 6], $bytes[$offset + 7]]) };
    (u64, $bytes:expr, $offset:expr) => { u64::from_le_bytes([$bytes[$offset], $bytes[$offset + 1], $bytes[$offset + 2], $bytes[$offset + 3], $bytes[$offset + 4], $bytes[$offset + 5], $bytes[$offset + 6], $bytes[$offset + 7]]) };
    (f32, $bytes:expr, $offset:expr) => { f32::from_bits(u32::from_le_bytes([$bytes[$offset], $bytes[$offset + 1], $bytes[$offset + 2], $bytes[$offset + 3]])) };
    (f64, $bytes:expr, $offset:expr) => { f64::from_bits(u64::from_le_bytes([$bytes[$offset], $bytes[$offset + 1], $bytes[$offset + 2], $bytes[$offset + 3], $bytes[$offset + 4], $bytes[$offset + 5], $bytes[$offset + 6], $bytes[$offset + 7]])) };
}

#[rustfmt::skip]
macro_rules! lane_write {
    (i8,  $value:expr) => { [$value as u8] };
    (u8,  $value:expr) => { [$value] };
    (i16, $value:expr) => { $value.to_le_bytes() };
    (u16, $value:expr) => { $value.to_le_bytes() };
    (i32, $value:expr) => { $value.to_le_bytes() };
    (u32, $value:expr) => { $value.to_le_bytes() };
    (i64, $value:expr) => { $value.to_le_bytes() };
    (u64, $value:expr) => { $value.to_le_bytes() };
    (f32, $value:expr) => { $value.to_bits().to_le_bytes() };
    (f64, $value:expr) => { $value.to_bits().to_le_bytes() };
}

macro_rules! impl_lane_accessors {
    ($( $as_vis:vis $as_name:ident => $from_vis:vis $from_name:ident : $lane_ty:tt, $lane_count:expr, $lane_bytes:expr; )*) => {
        $(
            #[inline]
            $as_vis const fn $as_name(self) -> [$lane_ty; $lane_count] {
                let bytes = self.0;
                let mut out = [0 as $lane_ty; $lane_count];
                let mut i = 0;
                while i < $lane_count {
                    out[i] = lane_read!($lane_ty, bytes, i * $lane_bytes);
                    i += 1;
                }
                out
            }

            #[inline]
            $from_vis const fn $from_name(lanes: [$lane_ty; $lane_count]) -> Self {
                let mut bytes = [0u8; 16];
                let mut i = 0;
                while i < $lane_count {
                    let lane = lane_write!($lane_ty, lanes[i]);
                    let mut j = 0;
                    while j < $lane_bytes {
                        bytes[i * $lane_bytes + j] = lane[j];
                        j += 1;
                    }
                    i += 1;
                }
                Self(bytes)
            }
        )*
    };
}
