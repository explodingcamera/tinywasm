mod interpreter;
pub(crate) mod stack;

#[doc(hidden)]
pub use stack::values;
pub use stack::values::*;

pub(crate) use stack::{CallFrame, Stack};

/// The main TinyWasm runtime.
///
/// This is the default runtime used by TinyWasm.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}
