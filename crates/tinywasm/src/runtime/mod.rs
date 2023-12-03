mod executer;
mod stack;

pub use executer::*;
pub use stack::*;

/// A WebAssembly Runtime.
/// See https://webassembly.github.io/spec/core/exec/runtime.html
#[derive(Debug, Default)]
pub struct Runtime {
    pub stack: Stack,
}
