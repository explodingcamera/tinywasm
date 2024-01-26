pub(super) trait FExt {
    fn round(self) -> Self;
    fn abs(self) -> Self;
    fn signum(self) -> Self;
    fn ceil(self) -> Self;
    fn floor(self) -> Self;
    fn trunc(self) -> Self;
    fn sqrt(self) -> Self;
    fn copysign(self, other: Self) -> Self;
}

impl FExt for f64 {
    fn round(self) -> Self {
        libm::round(self)
    }

    fn abs(self) -> Self {
        libm::fabs(self)
    }

    fn signum(self) -> Self {
        libm::copysign(1.0, self)
    }

    fn ceil(self) -> Self {
        libm::ceil(self)
    }

    fn floor(self) -> Self {
        libm::floor(self)
    }

    fn trunc(self) -> Self {
        libm::trunc(self)
    }

    fn sqrt(self) -> Self {
        libm::sqrt(self)
    }

    fn copysign(self, other: Self) -> Self {
        libm::copysign(self, other)
    }
}
impl FExt for f32 {
    fn round(self) -> Self {
        libm::roundf(self)
    }

    fn abs(self) -> Self {
        libm::fabsf(self)
    }

    fn signum(self) -> Self {
        libm::copysignf(1.0, self)
    }

    fn ceil(self) -> Self {
        libm::ceilf(self)
    }

    fn floor(self) -> Self {
        libm::floorf(self)
    }

    fn trunc(self) -> Self {
        libm::truncf(self)
    }

    fn sqrt(self) -> Self {
        libm::sqrtf(self)
    }

    fn copysign(self, other: Self) -> Self {
        libm::copysignf(self, other)
    }
}
