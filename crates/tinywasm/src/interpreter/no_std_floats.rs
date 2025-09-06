pub(super) trait NoStdFloatExt {
    fn round(self) -> Self;
    fn ceil(self) -> Self;
    fn floor(self) -> Self;
    fn trunc(self) -> Self;
    fn sqrt(self) -> Self;
}

#[rustfmt::skip]
impl NoStdFloatExt for f64 {
    #[inline] fn round(self) -> Self { libm::round(self) }
    #[inline] fn ceil(self) -> Self { libm::ceil(self) }
    #[inline] fn floor(self) -> Self { libm::floor(self) }
    #[inline] fn trunc(self) -> Self { libm::trunc(self) }
    #[inline] fn sqrt(self) -> Self { libm::sqrt(self) }
}

#[rustfmt::skip]
impl NoStdFloatExt for f32 {
    #[inline] fn round(self) -> Self { libm::roundf(self) }
    #[inline] fn ceil(self) -> Self { libm::ceilf(self) }
    #[inline] fn floor(self) -> Self { libm::floorf(self) }
    #[inline] fn trunc(self) -> Self { libm::truncf(self) }
    #[inline] fn sqrt(self) -> Self { libm::sqrtf(self) }
}
