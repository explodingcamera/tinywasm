use crate::{ExternAddr, FuncAddr};

const NULL_REF: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ExternRef(u32);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FuncRef(u32);

#[cfg(feature = "debug")]
impl core::fmt::Debug for ExternRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.addr() {
            Some(addr) => write!(f, "extern({addr:?})"),
            None => write!(f, "extern(null)"),
        }
    }
}

#[cfg(feature = "debug")]
impl core::fmt::Debug for FuncRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.addr() {
            Some(addr) => write!(f, "func({addr:?})"),
            None => write!(f, "func(null)"),
        }
    }
}

impl FuncRef {
    #[inline]
    /// Create a new [`FuncRef`] from a [`FuncAddr`].
    pub const fn new(addr: Option<FuncAddr>) -> Self {
        match addr {
            Some(addr) => Self(addr),
            None => Self::null(),
        }
    }

    #[inline]
    /// Create a null [`FuncRef`].
    pub const fn null() -> Self {
        Self(NULL_REF)
    }

    #[inline]
    /// Check if the [`FuncRef`] is null.
    pub const fn is_null(&self) -> bool {
        self.0 == NULL_REF
    }

    #[inline]
    /// Get the [`FuncAddr`] from the [`FuncRef`].
    pub const fn addr(&self) -> Option<FuncAddr> {
        if self.is_null() { None } else { Some(self.0) }
    }

    #[inline]
    #[doc(hidden)]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    #[doc(hidden)]
    pub const fn raw(&self) -> u32 {
        self.0
    }
}

impl ExternRef {
    #[inline]
    /// Create a new [`ExternRef`] from an [`ExternAddr`].
    /// Should only be used by the runtime.
    pub const fn new(addr: Option<ExternAddr>) -> Self {
        match addr {
            Some(addr) => Self(addr),
            None => Self::null(),
        }
    }

    /// Create a null [`ExternRef`].
    #[inline]
    pub const fn null() -> Self {
        Self(NULL_REF)
    }

    /// Check if the [`ExternRef`] is null.
    #[inline]
    pub const fn is_null(&self) -> bool {
        self.0 == NULL_REF
    }

    /// Get the [`ExternAddr`] from the [`ExternRef`].
    #[inline]
    pub const fn addr(&self) -> Option<ExternAddr> {
        if self.is_null() { None } else { Some(self.0) }
    }

    #[inline]
    #[doc(hidden)]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    #[doc(hidden)]
    pub const fn raw(&self) -> u32 {
        self.0
    }
}
