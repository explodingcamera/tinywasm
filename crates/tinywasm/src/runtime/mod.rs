mod interpreter;
mod stack;

mod raw;

#[cfg(all(not(feature = "nightly"), feature = "simd"))]
compile_error!("`simd` feature requires nightly");

#[cfg(feature = "simd")]
mod raw_simd;

use crate::Result;

pub use raw::RawWasmValue;
pub(crate) use stack::CallFrame;
pub(crate) use stack::Stack;

#[allow(rustdoc::private_intra_doc_links)]
/// A WebAssembly runtime.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html>
pub trait Runtime {
    /// Execute all call-frames on the stack until the stack is empty.
    fn exec(&self, store: &mut crate::Store, stack: &mut Stack) -> Result<()>;
}

/// The main TinyWasm runtime.
///
/// This is the default runtime used by TinyWasm.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}
