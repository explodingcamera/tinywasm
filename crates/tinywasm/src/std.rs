#[cfg(not(feature = "std"))]
pub(crate) use core::*;

#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "std")]
pub(crate) use std::*;

pub(crate) mod error {
    #[cfg(feature = "std")]
    extern crate std;

    #[cfg(feature = "std")]
    pub(crate) use std::error::Error;

    #[cfg(all(not(feature = "std"), nightly))]
    pub(crate) use core::error::Error;
}
