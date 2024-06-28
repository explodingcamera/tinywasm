mod executor;
mod num_helpers;
pub(crate) mod stack;

#[doc(hidden)]
pub use stack::values;
pub use stack::values::*;

pub(crate) use stack::{CallFrame, Stack};

use crate::{Result, Store};

/// The main TinyWasm runtime.
///
/// This is the default runtime used by TinyWasm.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}

impl InterpreterRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut Stack) -> Result<()> {
        executor::Executor::new(store, stack)?.run_to_completion()
    }
}
