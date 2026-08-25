#[cfg(not(feature = "send"))]
pub(crate) use alloc::rc::Rc as StoreShared;
#[cfg(feature = "send")]
pub(crate) use tinywasm_types::Shared as StoreShared;

#[cfg(not(feature = "portable-atomic"))]
pub(crate) use core::sync::atomic::{AtomicU32, Ordering};
#[cfg(feature = "portable-atomic")]
pub(crate) use portable_atomic::{AtomicU32, Ordering};
