pub(super) trait NoStdFloatExt {
    fn round(self) -> Self;
    fn abs(self) -> Self;
    fn signum(self) -> Self;
    fn ceil(self) -> Self;
    fn floor(self) -> Self;
    fn trunc(self) -> Self;
    fn sqrt(self) -> Self;
    fn copysign(self, other: Self) -> Self;
}

#[rustfmt::skip]
impl NoStdFloatExt for f64 {
    #[inline] fn round(self) -> Self { libm::round(self) }
    #[inline] fn abs(self) -> Self { libm::fabs(self) }
    #[inline] fn signum(self) -> Self { libm::copysign(1.0, self) }
    #[inline] fn ceil(self) -> Self { libm::ceil(self) }
    #[inline] fn floor(self) -> Self { libm::floor(self) }
    #[inline] fn trunc(self) -> Self { libm::trunc(self) }
    #[inline] fn sqrt(self) -> Self { libm::sqrt(self) }
    #[inline] fn copysign(self, other: Self) -> Self { libm::copysign(self, other) }
}

#[rustfmt::skip]
impl NoStdFloatExt for f32 {
    #[inline] fn round(self) -> Self { libm::roundf(self) }
    #[inline] fn abs(self) -> Self { libm::fabsf(self) }
    #[inline] fn signum(self) -> Self { libm::copysignf(1.0, self) }
    #[inline] fn ceil(self) -> Self { libm::ceilf(self) }
    #[inline] fn floor(self) -> Self { libm::floorf(self) }
    #[inline] fn trunc(self) -> Self { libm::truncf(self) }
    #[inline] fn sqrt(self) -> Self { libm::sqrtf(self) }
    #[inline] fn copysign(self, other: Self) -> Self { libm::copysignf(self, other) }
}
