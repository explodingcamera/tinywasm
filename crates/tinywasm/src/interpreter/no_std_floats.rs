// see https://github.com/rust-lang/rust/issues/137578 :(
pub(super) trait NoStdFloatExt {
    fn round(self) -> Self;
    fn ceil(self) -> Self;
    fn floor(self) -> Self;
    fn trunc(self) -> Self;
    fn sqrt(self) -> Self;
}

#[rustfmt::skip]
impl NoStdFloatExt for f64 {
    fn round(self) -> Self { libm::round(self) }
    fn ceil(self) -> Self { libm::ceil(self) }
    fn floor(self) -> Self { libm::floor(self) }
    fn trunc(self) -> Self { libm::trunc(self) }
    fn sqrt(self) -> Self { libm::sqrt(self) }
}

#[rustfmt::skip]
impl NoStdFloatExt for f32 {
    fn round(self) -> Self { libm::roundf(self) }
    fn ceil(self) -> Self { libm::ceilf(self) }
    fn floor(self) -> Self { libm::floorf(self) }
    fn trunc(self) -> Self { libm::truncf(self) }
    fn sqrt(self) -> Self { libm::sqrtf(self) }
}
