mod executer;
mod stack;
mod types;

pub use executer::*;
pub use stack::*;
pub use types::*;

/// A WebAssembly Runtime.
/// See https://webassembly.github.io/spec/core/exec/runtime.html
#[derive(Debug, Default)]
pub struct Runtime {
    pub stack: Stack,
}
