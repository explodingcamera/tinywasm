mod block_stack;
mod call_stack;
mod value_stack;

use self::{call_stack::CallStack, value_stack::ValueStack};
pub(crate) use block_stack::{BlockFrame, BlockStack, BlockType};
pub(crate) use call_stack::CallFrame;

/// A WebAssembly Stack
#[derive(Debug)]
pub struct Stack {
    pub(crate) values: ValueStack,
    pub(crate) blocks: BlockStack,
    pub(crate) call_stack: CallStack,
}

impl Stack {
    pub(crate) fn new(call_frame: CallFrame) -> Self {
        Self { values: ValueStack::default(), blocks: BlockStack::default(), call_stack: CallStack::new(call_frame) }
    }
}
