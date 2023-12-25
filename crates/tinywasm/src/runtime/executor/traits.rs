pub(crate) trait CheckedWrappingRem
where
    Self: Sized,
{
    fn checked_wrapping_rem(self, rhs: Self) -> Option<Self>;
}

pub(crate) trait WasmFloatOps {
    fn wasm_min(self, other: Self) -> Self;
    fn wasm_max(self, other: Self) -> Self;
    fn wasm_nearest(self) -> Self;
}

macro_rules! impl_wasm_float_ops {
    ($($t:ty)*) => ($(
        impl WasmFloatOps for $t {
            // https://webassembly.github.io/spec/core/exec/numerics.html#op-fnearest
            fn wasm_nearest(self) -> Self {
                log::info!("wasm_nearest: {}", self);
                match self {
                    x if x.is_nan() => x,
                    x if x.is_infinite() || x == 0.0 => x,
                    x if x > 0.0 && x <= 0.5 => 0.0,
                    x if x < 0.0 && x >= -0.5 => -0.0,
                    x => x.round(),
                }
            }

            // https://webassembly.github.io/spec/core/exec/numerics.html#op-fmin
            // Based on f32::minimum (which is not yet stable)
            #[inline]
            fn wasm_min(self, other: Self) -> Self {
                if self < other {
                    self
                } else if other < self {
                    other
                } else if self == other {
                    if self.is_sign_negative() && other.is_sign_positive() { self } else { other }
                } else {
                    // At least one input is NaN. Use `+` to perform NaN propagation and quieting.
                    self + other
                }
            }

            // https://webassembly.github.io/spec/core/exec/numerics.html#op-fmax
            // Based on f32::maximum (which is not yet stable)
            #[inline]
            fn wasm_max(self, other: Self) -> Self {
                if self > other {
                    self
                } else if other > self {
                    other
                } else if self == other {
                    if self.is_sign_negative() && other.is_sign_positive() { other } else { self }
                } else {
                    // At least one input is NaN. Use `+` to perform NaN propagation and quieting.
                    self + other
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
