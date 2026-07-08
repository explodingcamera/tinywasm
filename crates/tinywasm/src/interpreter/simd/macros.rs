#![allow(unused_macros)]

macro_rules! simd_impl {
    ($(wasm => $wasm:block)? $(x86 => $x86:block)? generic => $generic:block) => {
        cfg_select! {
            any(target_arch = "wasm32", target_arch = "wasm64") => { simd_impl!(@pick_wasm $( $wasm )? ; $generic) },
            all(feature = "simd-x86", target_arch = "x86_64", target_feature = "sse4.2", target_feature = "avx", target_feature = "avx2", target_feature = "bmi1", target_feature = "bmi2", target_feature = "fma", target_feature = "lzcnt", target_feature = "movbe", target_feature = "popcnt") => { simd_impl!(@pick_x86 $( $x86 )? ; $generic) },
            _ => { $generic }
        }
    };
    (@pick_wasm $wasm:block ; $generic:block) => { $wasm };
    (@pick_wasm ; $generic:block) => { $generic };
    (@pick_x86 $x86:block ; $generic:block) => { $x86 };
    (@pick_x86 ; $generic:block) => { $generic };
}

macro_rules! simd_binop {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $as_lanes:ident, $from_lanes:ident, $op:ident) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => {
                let a = $lhs.$as_lanes();
                let b = $rhs.$as_lanes();
                Self::$from_lanes(core::array::from_fn(|i| a[i].$op(b[i])))
            }
        }
    }};
}

macro_rules! simd_shift {
    ($value:expr, $shift:expr, $as_lanes:ident, $from_lanes:ident, $mask:expr, shl) => {{
        let lanes = $value.$as_lanes();
        let s = $shift & $mask;
        Self::$from_lanes(core::array::from_fn(|i| lanes[i].wrapping_shl(s)))
    }};
    ($value:expr, $shift:expr, $as_lanes:ident, $from_lanes:ident, $mask:expr, shr) => {{
        let lanes = $value.$as_lanes();
        let s = $shift & $mask;
        Self::$from_lanes(core::array::from_fn(|i| lanes[i] >> s))
    }};
}

macro_rules! simd_avgr_u {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $lane_ty:ty, $wide_ty:ty, $as_lanes:ident, $from_lanes:ident) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => {
                let a = $lhs.$as_lanes();
                let b = $rhs.$as_lanes();
                Self::$from_lanes(core::array::from_fn(|i| ((a[i] as $wide_ty + b[i] as $wide_ty + 1) >> 1) as $lane_ty))
            }
        }
    }};
}

macro_rules! simd_extend_cast {
    ($value:expr, $src_as:ident, $dst_from:ident, $dst_ty:ty, $offset:expr) => {{
        let lanes = $value.$src_as();
        Self::$dst_from(core::array::from_fn(|i| lanes[$offset + i] as $dst_ty))
    }};
}

macro_rules! simd_extmul {
    ($lhs:expr, $rhs:expr, $src_as:ident, $dst_from:ident, $dst_ty:ty, $offset:expr) => {{
        let a = $lhs.$src_as();
        let b = $rhs.$src_as();
        Self::$dst_from(core::array::from_fn(|i| (a[$offset + i] as $dst_ty).wrapping_mul(b[$offset + i] as $dst_ty)))
    }};
}

macro_rules! simd_cmp_mask {
    (generic $lhs:expr, $rhs:expr, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        let a = $lhs.$as_lanes();
        let b = $rhs.$as_lanes();
        Self::$from_lanes(core::array::from_fn(|i| if a[i] $cmp b[i] { -1 } else { 0 }))
    }};
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => { simd_cmp_mask!(generic $lhs, $rhs, $as_lanes, $from_lanes, $cmp) }
        }
    }};
}

macro_rules! simd_abs_const {
    ($value:expr, $as_lanes:ident, $from_lanes:ident) => {{
        let a = $value.$as_lanes();
        Self::$from_lanes(core::array::from_fn(|i| a[i].wrapping_abs()))
    }};
}

macro_rules! simd_neg {
    ($value:expr, $wasm_op:ident, $as_lanes:ident, $from_lanes:ident) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($value.to_wasm_v128())) }
            generic => {
                let a = $value.$as_lanes();
                Self::$from_lanes(core::array::from_fn(|i| a[i].wrapping_neg()))
            }
        }
    }};
}

macro_rules! simd_minmax {
    ($lhs:expr, $rhs:expr, $wasm_op:ident, $as_lanes:ident, $from_lanes:ident, $cmp:tt) => {{
        simd_impl! {
            wasm => { Self::from_wasm_v128(wasm::$wasm_op($lhs.to_wasm_v128(), $rhs.to_wasm_v128())) }
            generic => {
                let a = $lhs.$as_lanes();
                let b = $rhs.$as_lanes();
                Self::$from_lanes(core::array::from_fn(|i| if a[i] $cmp b[i] { a[i] } else { b[i] }))
            }
        }
    }};
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
    ($($as_vis:vis $as_name:ident => $from_vis:vis $from_name:ident: $lane_ty:tt, $lane_count:expr, $lane_bytes:expr;)*) => {
        $(
            #[inline]
            $as_vis const fn $as_name(self) -> [$lane_ty; $lane_count] {
                const { assert!($lane_count * $lane_bytes == 16); };
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
                const { assert!($lane_count * $lane_bytes == 16); };

                let mut bytes = [0u8; 16];
                let mut i = 0;
                while i < $lane_count {
                    let offset = i * $lane_bytes;
                    let lane = lane_write!($lane_ty, lanes[i]);
                    let mut j = 0;
                    while j < $lane_bytes {
                        bytes[offset + j] = lane[j];
                        j += 1;
                    }
                    i += 1;
                }
                Self(bytes)
            }
        )*
    };
}
