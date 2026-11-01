//! WebAssembly numeric operations.

mod float;
mod int;
#[cfg(any(test, all(not(feature = "std"), not(tw_core_float_math))))]
mod sqrt;

pub(crate) use float::{f32, f64};
pub(crate) use int::{i16, i32, i64, u32, u64};
