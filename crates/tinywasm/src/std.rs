#[cfg(not(feature = "std"))]
pub use core::*;

#[cfg(feature = "std")]
extern crate std;
#[cfg(feature = "std")]
pub use std::*;

pub mod error {
    #[cfg(feature = "std")]
    extern crate std;

    #[cfg(feature = "std")]
    pub use std::error::Error;

    #[cfg(not(feature = "std"))]
    pub use core::error::Error;
}
