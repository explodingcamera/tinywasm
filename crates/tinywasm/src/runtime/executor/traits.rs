pub(crate) trait CheckedWrappingRem
where
    Self: Sized,
{
    fn checked_wrapping_rem(self, rhs: Self) -> Option<Self>;
}

pub(crate) trait WrappingSelfOps {
    fn wrapping_shl_self(self, rhs: Self) -> Self;
    fn wrapping_shr_self(self, rhs: Self) -> Self;
    fn wrapping_rotl_self(self, rhs: Self) -> Self;
    fn wrapping_rotr_self(self, rhs: Self) -> Self;
}

macro_rules! impl_wrapping_self_sh {
    ($($t:ty)*) => ($(
        impl WrappingSelfOps for $t {
            #[inline]
            fn wrapping_shl_self(self, rhs: Self) -> Self {
                self.wrapping_shl(rhs as u32)
            }

            #[inline]
            fn wrapping_shr_self(self, rhs: Self) -> Self {
                self.wrapping_shr(rhs as u32)
            }

            #[inline]
            fn wrapping_rotl_self(self, rhs: Self) -> Self {
                self.rotate_left(rhs as u32)
            }

            #[inline]
            fn wrapping_rotr_self(self, rhs: Self) -> Self {
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
