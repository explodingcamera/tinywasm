pub(crate) trait TinywasmIntExt
where
    Self: Sized,
{
    fn checked_wrapping_rem(self, rhs: Self) -> Result<Self>;
    fn wasm_checked_div(self, rhs: Self) -> Result<Self>;
}

/// Doing the actual conversion from float to int is a bit tricky, because
/// we need to check for overflow. This macro generates the min/max values
/// for a specific conversion, which are then used in the actual conversion.
/// Rust sadly doesn't have wrapping casts for floats yet, maybe never.
/// Alternatively, https://crates.io/crates/az could be used for this but
/// it's not worth the dependency. 
#[rustfmt::skip] 
macro_rules! float_min_max {
    (f32, i32) => {(-2147483904.0_f32, 2147483648.0_f32)};
    (f64, i32) => {(-2147483649.0_f64, 2147483648.0_f64)};
    (f32, u32) => {(-1.0_f32, 4294967296.0_f32)}; // 2^32
    (f64, u32) => {(-1.0_f64, 4294967296.0_f64)}; // 2^32
    (f32, i64) => {(-9223373136366403584.0_f32, 9223372036854775808.0_f32)}; // 2^63 + 2^40 | 2^63
    (f64, i64) => {(-9223372036854777856.0_f64, 9223372036854775808.0_f64)}; // 2^63 + 2^40 | 2^63
    (f32, u64) => {(-1.0_f32, 18446744073709551616.0_f32)}; // 2^64
    (f64, u64) => {(-1.0_f64, 18446744073709551616.0_f64)}; // 2^64
    // other conversions are not allowed
    ($from:ty, $to:ty) => {compile_error!("invalid float conversion")};
}

/// Convert a value on the stack with error checking
macro_rules! checked_conv_float {
    // Direct conversion with error checking (two types)
    ($from:tt, $to:tt, $self:expr) => {
        checked_conv_float!($from, $to, $to, $self)
    };
    // Conversion with an intermediate unsigned type and error checking (three types)
    ($from:tt, $intermediate:tt, $to:tt, $self:expr) => {
        $self
            .stack
            .values
            .replace_top::<$from, $to>(|v| {
                let (min, max) = float_min_max!($from, $intermediate);
                if unlikely(v.is_nan()) {
                    return Err(Error::Trap(crate::Trap::InvalidConversionToInt));
                }
                if unlikely(v <= min || v >= max) {
                    return Err(Error::Trap(crate::Trap::IntegerOverflow));
                }
                Ok((v as $intermediate as $to).into())
            })
            .to_cf()?
    };
}

pub(crate) use checked_conv_float;
pub(crate) use float_min_max;

pub(super) fn trap_0() -> Error {
    Error::Trap(crate::Trap::DivisionByZero)
}
pub(crate) trait TinywasmFloatExt {
    fn tw_minimum(self, other: Self) -> Self;
    fn tw_maximum(self, other: Self) -> Self;
    fn tw_nearest(self) -> Self;
}

use crate::{Error, Result};

#[cfg(not(feature = "std"))]
use super::no_std_floats::NoStdFloatExt;

macro_rules! impl_wasm_float_ops {
    ($($t:ty)*) => ($(
        impl TinywasmFloatExt for $t {
            // https://webassembly.github.io/spec/core/exec/numerics.html#op-fnearest
            fn tw_nearest(self) -> Self {
                match self {
                    x if x.is_nan() => x, // preserve NaN
                    x if x.is_infinite() || x == 0.0 => x, // preserve infinities and zeros
                    x if (0.0..=0.5).contains(&x) => 0.0,
                    x if (-0.5..0.0).contains(&x) => -0.0,
                    x => {
                        // Handle normal and halfway cases
                        let rounded = x.round();
                        let diff = (x - rounded).abs();
                        if diff != 0.5 || rounded % 2.0 == 0.0 {
                            return rounded
                        }

                        rounded - x.signum() // Make even
                    }
                }
            }

            // https://webassembly.github.io/spec/core/exec/numerics.html#op-fmin
            // Based on f32::minimum (which is not yet stable)
            #[inline]
            fn tw_minimum(self, other: Self) -> Self {
                match self.partial_cmp(&other) {
                    Some(core::cmp::Ordering::Less) => self,
                    Some(core::cmp::Ordering::Greater) => other,
                    Some(core::cmp::Ordering::Equal) => if self.is_sign_negative() && other.is_sign_positive() { self } else { other },
                    None => self + other, // At least one input is NaN. Use `+` to perform NaN propagation and quieting.
                }
            }

            // https://webassembly.github.io/spec/core/exec/numerics.html#op-fmax
            // Based on f32::maximum (which is not yet stable)
            #[inline]
            fn tw_maximum(self, other: Self) -> Self {
                match self.partial_cmp(&other) {
                    Some(core::cmp::Ordering::Greater) => self,
                    Some(core::cmp::Ordering::Less) => other,
                    Some(core::cmp::Ordering::Equal) => if self.is_sign_negative() && other.is_sign_positive() { other } else { self },
                    None => self + other, // At least one input is NaN. Use `+` to perform NaN propagation and quieting.
                }
            }
        }
    )*)
}

impl_wasm_float_ops! { f32 f64 }

pub(crate) trait WasmIntOps {
    fn wasm_shl(self, rhs: Self) -> Self;
    fn wasm_shr(self, rhs: Self) -> Self;
    fn wasm_rotl(self, rhs: Self) -> Self;
    fn wasm_rotr(self, rhs: Self) -> Self;
}

macro_rules! impl_wrapping_self_sh {
    ($($t:ty)*) => ($(
        impl WasmIntOps for $t {
            #[inline]
            fn wasm_shl(self, rhs: Self) -> Self {
                self.wrapping_shl(rhs as u32)
            }

            #[inline]
            fn wasm_shr(self, rhs: Self) -> Self {
                self.wrapping_shr(rhs as u32)
            }

            #[inline]
            fn wasm_rotl(self, rhs: Self) -> Self {
                self.rotate_left(rhs as u32)
            }

            #[inline]
            fn wasm_rotr(self, rhs: Self) -> Self {
                self.rotate_right(rhs as u32)
            }
        }
    )*)
}

impl_wrapping_self_sh! { i32 i64 u32 u64 }

macro_rules! impl_checked_wrapping_rem {
    ($($t:ty)*) => ($(
        impl TinywasmIntExt for $t {
            #[inline]
            fn checked_wrapping_rem(self, rhs: Self) -> Result<Self> {
                if rhs == 0 {
                    Err(Error::Trap(crate::Trap::DivisionByZero))
                } else {
                    Ok(self.wrapping_rem(rhs))
                }
            }

            #[inline]
            fn wasm_checked_div(self, rhs: Self) -> Result<Self> {
                if rhs == 0 {
                    Err(Error::Trap(crate::Trap::DivisionByZero))
                } else {
                    self.checked_div(rhs).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
                }
            }
        }
    )*)
}

impl_checked_wrapping_rem! { i32 i64 u32 u64 }
