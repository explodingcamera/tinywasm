use super::Value128;

#[cfg(not(feature = "std"))]
use crate::interpreter::no_std_floats::NoStdFloatExt;

impl Value128 {
    pub(super) fn extract_lane_bytes<const LANE_BYTES: usize>(self, lane: u8, lane_count: u8) -> [u8; LANE_BYTES] {
        debug_assert!(lane < lane_count);
        let bytes = self.0;
        let start = lane as usize * LANE_BYTES;
        let mut out = [0u8; LANE_BYTES];
        out.copy_from_slice(&bytes[start..start + LANE_BYTES]);
        out
    }

    pub(super) fn replace_lane_bytes<const LANE_BYTES: usize>(
        self,
        lane: u8,
        value: [u8; LANE_BYTES],
        lane_count: u8,
    ) -> Self {
        debug_assert!(lane < lane_count);
        let mut bytes = self.0;
        let start = lane as usize * LANE_BYTES;
        bytes[start..start + LANE_BYTES].copy_from_slice(&value);
        Self(bytes)
    }
}

pub(super) const fn canonicalize_simd_f32_nan(x: f32) -> f32 {
    #[cfg(feature = "canonicalize_nans")]
    if x.is_nan() {
        f32::NAN
    } else {
        x
    }
    #[cfg(not(feature = "canonicalize_nans"))]
    x
}

pub(super) const fn canonicalize_simd_f64_nan(x: f64) -> f64 {
    #[cfg(feature = "canonicalize_nans")]
    if x.is_nan() {
        f64::NAN
    } else {
        x
    }
    #[cfg(not(feature = "canonicalize_nans"))]
    x
}

pub(super) const fn saturate_i16_to_i8(x: i16) -> i8 {
    match x {
        v if v > i8::MAX as i16 => i8::MAX,
        v if v < i8::MIN as i16 => i8::MIN,
        v => v as i8,
    }
}

pub(super) const fn saturate_i16_to_u8(x: i16) -> u8 {
    match x {
        v if v <= 0 => 0,
        v if v > u8::MAX as i16 => u8::MAX,
        v => v as u8,
    }
}

pub(super) const fn saturate_i32_to_i16(x: i32) -> i16 {
    match x {
        v if v > i16::MAX as i32 => i16::MAX,
        v if v < i16::MIN as i32 => i16::MIN,
        v => v as i16,
    }
}

pub(super) const fn saturate_i32_to_u16(x: i32) -> u16 {
    match x {
        v if v <= 0 => 0,
        v if v > u16::MAX as i32 => u16::MAX,
        v => v as u16,
    }
}

pub(super) fn trunc_sat_f32_to_i32(v: f32) -> i32 {
    match v {
        x if x.is_nan() => 0,
        x if x <= i32::MIN as f32 - (1 << 8) as f32 => i32::MIN,
        x if x >= (i32::MAX as f32 + 1.0) => i32::MAX,
        x => x.trunc() as i32,
    }
}

pub(super) fn trunc_sat_f32_to_u32(v: f32) -> u32 {
    match v {
        x if x.is_nan() || x <= -1.0_f32 => 0,
        x if x >= (u32::MAX as f32 + 1.0) => u32::MAX,
        x => x.trunc() as u32,
    }
}

pub(super) fn trunc_sat_f64_to_i32(v: f64) -> i32 {
    match v {
        x if x.is_nan() => 0,
        x if x <= i32::MIN as f64 - 1.0_f64 => i32::MIN,
        x if x >= (i32::MAX as f64 + 1.0) => i32::MAX,
        x => x.trunc() as i32,
    }
}

pub(super) fn trunc_sat_f64_to_u32(v: f64) -> u32 {
    match v {
        x if x.is_nan() || x <= -1.0_f64 => 0,
        x if x >= (u32::MAX as f64 + 1.0) => u32::MAX,
        x => x.trunc() as u32,
    }
}
