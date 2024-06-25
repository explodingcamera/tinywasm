mod interpreter;
mod stack;

pub(crate) use stack::{CallFrame, RawWasmValue, Stack};

/// The main TinyWasm runtime.
///
/// This is the default runtime used by TinyWasm.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}
