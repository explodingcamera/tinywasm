mod interpreter;
mod stack;

mod raw;

#[cfg(all(not(feature = "nightly"), feature = "simd"))]
compile_error!("`simd` feature requires nightly");

#[cfg(feature = "simd")]
mod raw_simd;

pub use raw::RawWasmValue;
pub(crate) use stack::CallFrame;
pub(crate) use stack::Stack;

/// The main TinyWasm runtime.
///
/// This is the default runtime used by TinyWasm.
#[derive(Debug, Default)]
pub struct InterpreterRuntime {}
