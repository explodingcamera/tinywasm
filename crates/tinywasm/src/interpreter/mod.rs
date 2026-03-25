pub(crate) mod executor;
pub(crate) mod num_helpers;
pub(crate) mod stack;
pub(crate) mod value128;
pub(crate) mod values;

#[cfg(not(feature = "std"))]
mod no_std_floats;

use crate::{Result, Store, interpreter::stack::CallFrame};
pub(crate) use value128::*;
pub(crate) use values::*;

/// The main `TinyWasm` runtime.
///
/// This is the default runtime used by `TinyWasm`.
#[derive(Debug, Default)]
pub(crate) struct InterpreterRuntime;

impl InterpreterRuntime {
    pub(crate) fn exec(store: &mut Store, cf: CallFrame) -> Result<()> {
        executor::Executor::new(store, cf)?.run_to_completion()
    }
}
