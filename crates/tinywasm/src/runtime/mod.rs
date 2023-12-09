mod executer;
mod stack;
mod value;

pub use stack::*;
pub(crate) use value::RawWasmValue;

/// A WebAssembly Runtime.
///
/// Generic over `CheckTypes` to enable type checking at runtime.
/// This is useful for debugging, but should be disabled if you know
/// that the module is valid.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html>
// Execution is implemented in the `executer` module
#[derive(Debug, Default)]
pub struct Runtime<const CHECK_TYPES: bool> {}
