use alloc::vec::Vec;

mod call;
pub use call::CallFrame;
use tinywasm_types::WasmValue;

// minimum stack size
pub const STACK_SIZE: usize = 1024;
// minimum call stack size
pub const CALL_STACK_SIZE: usize = 1024;

/// A WebAssembly Stack
#[derive(Debug)]
pub struct Stack {
    /// Locals
    // TODO: maybe store the locals on the stack instead?
    pub locals: Vec<WasmValue>,

    /// The value stack
    // TODO: Split into Vec<u8> and Vec<ValType> for better memory usage?
    pub value_stack: Vec<WasmValue>, // keeping this typed for now to make it easier to debug
    pub value_stack_top: usize,
    // /// The call stack
    // pub call_stack: Vec<CallFrame>,
    // pub call_stack_top: usize,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            locals: Vec::new(),
            value_stack: Vec::with_capacity(STACK_SIZE),
            value_stack_top: 0,
            // call_stack: Vec::with_capacity(CALL_STACK_SIZE),
            // call_stack_top: 0,
        }
    }
}
