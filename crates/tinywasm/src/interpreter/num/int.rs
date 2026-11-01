//! Trapping integer arithmetic and saturating narrowing.

macro_rules! impl_int_ops {
    ($int:ident) => {
        use crate::{Result, Trap};
        use core::primitive::$int as Int;

        /// Divides, trapping on zero divisors and signed overflow.
        #[inline]
        pub(crate) fn div(lhs: Int, rhs: Int) -> Result<Int, Trap> {
            if rhs == 0 {
                return Err(Trap::DivisionByZero);
            }
            lhs.checked_div(rhs).ok_or_else(|| Trap::IntegerOverflow)
        }

        /// Computes the remainder, trapping only on zero divisors.
        #[inline]
        pub(crate) fn rem(lhs: Int, rhs: Int) -> Result<Int, Trap> {
            if rhs == 0 {
                return Err(Trap::DivisionByZero);
            }
            Ok(lhs.wrapping_rem(rhs))
        }
    };
}

/// Signed 16-bit numeric operations.
pub(crate) mod i16 {
    /// Narrows to a signed byte, saturating at its limits.
    #[inline]
    pub(crate) const fn saturate_to_i8(value: i16) -> i8 {
        match value {
            v if v > i8::MAX as i16 => i8::MAX,
            v if v < i8::MIN as i16 => i8::MIN,
            v => v as i8,
        }
    }

    /// Narrows to an unsigned byte, saturating at its limits.
    #[inline]
    pub(crate) const fn saturate_to_u8(value: i16) -> u8 {
        match value {
            v if v <= 0 => 0,
            v if v > u8::MAX as i16 => u8::MAX,
            v => v as u8,
        }
    }
}

/// Signed 32-bit numeric operations.
pub(crate) mod i32 {
    impl_int_ops!(i32);

    /// Narrows to a signed 16-bit integer, saturating at its limits.
    #[inline]
    pub(crate) const fn saturate_to_i16(value: i32) -> i16 {
        match value {
            v if v > i16::MAX as i32 => i16::MAX,
            v if v < i16::MIN as i32 => i16::MIN,
            v => v as i16,
        }
    }

    /// Narrows to an unsigned 16-bit integer, saturating at its limits.
    #[inline]
    pub(crate) const fn saturate_to_u16(value: i32) -> u16 {
        match value {
            v if v <= 0 => 0,
            v if v > u16::MAX as i32 => u16::MAX,
            v => v as u16,
        }
    }
}

/// Signed 64-bit numeric operations.
pub(crate) mod i64 {
    impl_int_ops!(i64);
}

/// Unsigned 32-bit numeric operations.
pub(crate) mod u32 {
    impl_int_ops!(u32);
}

/// Unsigned 64-bit numeric operations.
pub(crate) mod u64 {
    impl_int_ops!(u64);
}
