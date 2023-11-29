use crate::WasmValue;
use alloc::vec::Vec;

mod call;
pub use call::CallFrame;

pub const STACK_SIZE: usize = 1024;

/// A WebAssembly Stack
pub struct Stack {
    /// Locals
    // TODO: maybe store the locals on the stack instead?
    pub locals: Vec<WasmValue>,

    /// The value stack
    // TODO: Split into Vec<u8> and Vec<ValType> for better memory usage?
    pub value_stack: Vec<WasmValue>, // keeping this typed for now to make it easier to debug
    pub value_stack_top: usize,

    /// The call stack
    pub call_stack: Vec<CallFrame>,
    pub call_stack_top: usize,
}
