pub(crate) trait CheckedWrappingRem
where
    Self: Sized,
{
    fn checked_wrapping_rem(self, rhs: Self) -> Option<Self>;
}

pub(crate) trait ShlI32 {
    fn shl_i32(self, rhs: i32) -> Self;
}

impl ShlI32 for i32 {
    #[inline]
    fn shl_i32(self, rhs: i32) -> Self {
        self.wrapping_shl(rhs as u32)
    }
}

pub(crate) trait ShlI64 {
    fn shl_i64(self, rhs: i64) -> Self;
}

impl ShlI64 for i64 {
    #[inline]
    fn shl_i64(self, rhs: i64) -> Self {
        self.wrapping_shl(rhs as u32)
    }
}

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

impl_checked_wrapping_rem! { i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize }
