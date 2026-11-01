//! WebAssembly floating-point operations with native and software implementations.

#[cfg(any(test, all(not(feature = "std"), not(tw_core_float_math))))]
macro_rules! impl_software_rounding {
    ($uint:ty, $integral_limit:expr) => {
        /// Rounds to the nearest integer with ties-to-even.
        #[inline]
        pub(super) fn software_nearest(value: Float) -> Float {
            let magnitude = value.abs();
            if magnitude >= $integral_limit {
                return value;
            }
            // Adding the limit rounds to an integer with ties-to-even. Restore negative zero.
            let rounded = (magnitude + $integral_limit) - $integral_limit;
            rounded.copysign(value)
        }

        /// Rounds toward positive infinity.
        #[inline]
        pub(super) fn software_ceil(value: Float) -> Float {
            let truncated = software_trunc(value);
            if value > truncated { truncated + 1.0 } else { truncated }
        }

        /// Rounds toward negative infinity.
        #[inline]
        pub(super) fn software_floor(value: Float) -> Float {
            let truncated = software_trunc(value);
            if value < truncated { truncated - 1.0 } else { truncated }
        }

        /// Rounds toward zero, preserving its sign and quieting NaNs.
        #[inline]
        pub(super) fn software_trunc(value: Float) -> Float {
            let bits = value.to_bits();
            let fraction_bits = Float::MANTISSA_DIGITS - 1;
            let sign = (1 as $uint) << (<$uint>::BITS - 1);
            let exponent_mask = (2 * Float::MAX_EXP - 1) as $uint;
            let exponent = ((bits >> fraction_bits) & exponent_mask) as i32 - (Float::MAX_EXP - 1);
            if exponent >= fraction_bits as i32 {
                return if value.is_nan() { value + value } else { value };
            }
            let mask = if exponent < 0 { sign } else { !((((1 as $uint) << fraction_bits) - 1) >> exponent as u32) };
            Float::from_bits(bits & mask)
        }
    };
}

macro_rules! impl_float_ops {
    ($float:ident, $sqrt:ident, $i32_step:expr, $i64_step:expr) => {
        use crate::{Result, Trap};
        use core::primitive::$float as Float;

        macro_rules! impl_checked_trunc {
            ($name:ident, $to:ty, $min:expr) => {
                /// Truncates to an integer, trapping on NaNs and out-of-range results.
                #[inline]
                pub(crate) fn $name(value: Float) -> Result<$to, Trap> {
                    if value.is_nan() {
                        core::hint::cold_path();
                        return Err(Trap::InvalidConversionToInt);
                    }
                    if value <= $min || value >= <$to>::MAX as Float + 1.0 {
                        core::hint::cold_path();
                        return Err(Trap::IntegerOverflow);
                    }
                    Ok(value as $to)
                }
            };
        }

        impl_checked_trunc!(trunc_to_i32, i32, i32::MIN as Float - $i32_step);
        impl_checked_trunc!(trunc_to_i64, i64, i64::MIN as Float - $i64_step);
        impl_checked_trunc!(trunc_to_u32, u32, -1.0);
        impl_checked_trunc!(trunc_to_u64, u64, -1.0);

        /// Replaces NaNs with a canonical NaN when canonicalization is enabled.
        #[inline]
        pub(crate) const fn canonicalize_nan(value: Float) -> Float {
            #[cfg(feature = "canonicalize-nans")]
            if value.is_nan() {
                return Float::NAN;
            }
            value
        }

        /// Rounds toward positive infinity.
        #[inline]
        pub(crate) fn ceil(value: Float) -> Float {
            cfg_select! {
                feature = "std" => value.ceil(),
                tw_core_float_math => core::$float::math::ceil(value),
                _ => software_ceil(value),
            }
        }

        /// Rounds toward negative infinity.
        #[inline]
        pub(crate) fn floor(value: Float) -> Float {
            cfg_select! {
                feature = "std" => value.floor(),
                tw_core_float_math => core::$float::math::floor(value),
                _ => software_floor(value),
            }
        }

        /// Rounds toward zero, preserving its sign.
        #[inline]
        pub(crate) fn trunc(value: Float) -> Float {
            cfg_select! {
                feature = "std" => value.trunc(),
                tw_core_float_math => core::$float::math::trunc(value),
                _ => software_trunc(value),
            }
        }

        /// Rounds to the nearest integer with ties-to-even.
        #[inline]
        pub(crate) fn nearest(value: Float) -> Float {
            #[cfg(feature = "canonicalize-nans")]
            if value.is_nan() {
                return Float::NAN;
            }
            cfg_select! {
                feature = "std" => value.round_ties_even(),
                tw_core_float_math => core::$float::math::round_ties_even(value),
                _ => software_nearest(value),
            }
        }

        /// Returns the correctly rounded square root, or NaN for negative nonzero inputs.
        #[inline]
        pub(crate) fn sqrt(value: Float) -> Float {
            cfg_select! {
                feature = "std" => value.sqrt(),
                tw_core_float_math => core::$float::math::sqrt(value),
                _ => crate::interpreter::num::sqrt::$sqrt(value),
            }
        }

        /// Implements Wasm minimum, including signed zeros and NaN propagation.
        #[inline]
        pub(crate) fn minimum(lhs: Float, rhs: Float) -> Float {
            match lhs.partial_cmp(&rhs) {
                Some(core::cmp::Ordering::Less) => lhs,
                Some(core::cmp::Ordering::Greater) => rhs,
                Some(core::cmp::Ordering::Equal) => {
                    if lhs.is_sign_negative() && rhs.is_sign_positive() {
                        lhs
                    } else {
                        rhs
                    }
                }
                #[cfg(not(feature = "canonicalize-nans"))]
                None => lhs + rhs,
                #[cfg(feature = "canonicalize-nans")]
                None => Float::NAN,
            }
        }

        /// Implements Wasm maximum, including signed zeros and NaN propagation.
        #[inline]
        pub(crate) fn maximum(lhs: Float, rhs: Float) -> Float {
            match lhs.partial_cmp(&rhs) {
                Some(core::cmp::Ordering::Greater) => lhs,
                Some(core::cmp::Ordering::Less) => rhs,
                Some(core::cmp::Ordering::Equal) => {
                    if lhs.is_sign_negative() && rhs.is_sign_positive() {
                        rhs
                    } else {
                        lhs
                    }
                }
                #[cfg(not(feature = "canonicalize-nans"))]
                None => lhs + rhs,
                #[cfg(feature = "canonicalize-nans")]
                None => Float::NAN,
            }
        }
    };
}

/// WebAssembly single-precision floating-point operations.
pub(crate) mod f32 {
    // Steps below signed integer minima give exclusive bounds at f32 precision.
    impl_float_ops!(f32, sqrt_f32, 256.0, (1i64 << 40) as f32);
    #[cfg(any(test, all(not(feature = "std"), not(tw_core_float_math))))]
    impl_software_rounding!(u32, 8_388_608.0_f32);
}

/// WebAssembly double-precision floating-point operations.
pub(crate) mod f64 {
    impl_float_ops!(f64, sqrt_f64, 1.0, 2048.0);
    #[cfg(any(test, all(not(feature = "std"), not(tw_core_float_math))))]
    impl_software_rounding!(u64, 4_503_599_627_370_496.0_f64);
}

#[cfg(all(test, feature = "std"))]
mod tests {
    macro_rules! check_float_ops {
        ($name:ident, $float:ident, $uint:ty, $sqrt:ident) => {
            #[test]
            fn $name() {
                let check = |bits: $uint| {
                    let value = <$float>::from_bits(bits);
                    for (actual, expected) in [
                        (super::$float::software_ceil(value), value.ceil()),
                        (super::$float::software_floor(value), value.floor()),
                        (super::$float::software_trunc(value), value.trunc()),
                        (super::$float::software_nearest(value), value.round_ties_even()),
                        (crate::interpreter::num::sqrt::$sqrt(value), value.sqrt()),
                    ] {
                        if expected.is_nan() {
                            assert!(actual.is_nan());
                            let quiet_bit = (1 as $uint) << (<$float>::MANTISSA_DIGITS - 2);
                            assert_ne!(actual.to_bits() & quiet_bit, 0);
                        } else {
                            assert_eq!(actual.to_bits(), expected.to_bits(), "input {bits:x}");
                        }
                    }
                };
                let sign = (1 as $uint) << (<$uint>::BITS - 1);
                for exponent in 0..(2 * <$float>::MAX_EXP) {
                    let center = (exponent as $uint) << (<$float>::MANTISSA_DIGITS - 1);
                    for offset in 0..=2 {
                        for bits in [center.wrapping_sub(offset), center.wrapping_add(offset)] {
                            check(bits);
                            check(bits | sign);
                        }
                    }
                }
                for value in -128..=128 {
                    let center = ((value as $float) * 0.5).to_bits();
                    for bits in [center.wrapping_sub(1), center, center.wrapping_add(1)] {
                        check(bits);
                    }
                }
                let mut state = 0x1234_5678_9abc_def0u64;
                for _ in 0..10_000 {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    check(state as $uint);
                }
            }
        };
    }

    check_float_ops!(matches_f32, f32, u32, sqrt_f32);
    check_float_ops!(matches_f64, f64, u64, sqrt_f64);
}
