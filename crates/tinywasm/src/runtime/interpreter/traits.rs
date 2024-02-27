pub(crate) trait CheckedWrappingRem
where
    Self: Sized,
{
    fn checked_wrapping_rem(self, rhs: Self) -> Option<Self>;
}

pub(crate) trait TinywasmFloatExt {
    fn tw_minimum(self, other: Self) -> Self;
    fn tw_maximum(self, other: Self) -> Self;
    fn tw_nearest(self) -> Self;
}

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
        impl CheckedWrappingRem for $t {
            #[inline]
            fn checked_wrapping_rem(self, rhs: Self) -> Option<Self> {
                if rhs == 0 {
                    None
                } else {
                    Some(self.wrapping_rem(rhs))
                }
            }
        }
    )*)
}

impl_checked_wrapping_rem! { i32 i64 u32 u64 }
