mod block_stack;
mod call_stack;
mod large_value_stack;
mod value_stack;

pub(crate) use block_stack::{BlockFrame, BlockStack, BlockType};
pub(crate) use call_stack::{CallFrame, CallStack};
pub(crate) use large_value_stack::LargeValueStack;
pub(crate) use value_stack::ValueStack;

use super::RawWasmValue;

/// A WebAssembly Stack
#[derive(Debug)]
pub struct Stack {
    pub(crate) values: ValueStack<RawWasmValue>,
    pub(crate) large_values: LargeValueStack,

    pub(crate) blocks: BlockStack,
    pub(crate) call_stack: CallStack,
}

impl Stack {
    pub(crate) fn new(call_frame: CallFrame) -> Self {
        Self {
            values: ValueStack::default(),
            blocks: BlockStack::new(),
            call_stack: CallStack::new(call_frame),
            large_values: LargeValueStack::default(),
        }
    }
}
