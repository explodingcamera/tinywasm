mod interpreter;
mod stack;
mod value;

use crate::Result;
pub use stack::*;
pub(crate) use value::RawWasmValue;

#[allow(rustdoc::private_intra_doc_links)]
/// A WebAssembly runtime.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html>
pub trait Runtime {
    /// Execute all call-frames on the stack until the stack is empty.
    fn exec(&self, store: &mut crate::Store, stack: &mut crate::runtime::Stack) -> Result<()>;
}

/// The main TinyWasm runtime.
///
/// This is the default runtime used by TinyWasm.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}
