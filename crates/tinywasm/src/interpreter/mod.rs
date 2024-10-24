pub(crate) mod executor;
pub(crate) mod num_helpers;
pub(crate) mod simd;
pub(crate) mod stack;
mod values;

#[cfg(not(feature = "std"))]
mod no_std_floats;

use crate::{Result, Store};
pub use values::*;

/// The main `TinyWasm` runtime.
///
/// This is the default runtime used by `TinyWasm`.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}

impl InterpreterRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut stack::Stack) -> Result<()> {
        executor::Executor::new(store, stack)?.run_to_completion()
    }
}
