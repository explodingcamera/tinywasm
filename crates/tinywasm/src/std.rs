#[cfg(not(feature = "std"))]
pub(crate) use core::*;

#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "std")]
pub(crate) use std::*;
