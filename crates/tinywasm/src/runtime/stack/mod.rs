mod block_stack;
mod call_stack;
mod value_stack;
pub mod values;

pub(crate) use block_stack::{BlockFrame, BlockStack, BlockType};
pub(crate) use call_stack::{CallFrame, CallStack};
pub(crate) use value_stack::{StackHeight, ValueStack};

/// A WebAssembly Stack
#[derive(Debug)]
pub(crate) struct Stack {
    pub(crate) values: ValueStack,
    pub(crate) blocks: BlockStack,
    pub(crate) call_stack: CallStack,
}

impl Stack {
    pub(crate) fn new(call_frame: CallFrame) -> Self {
        Self { values: ValueStack::new(), blocks: BlockStack::default(), call_stack: CallStack::new(call_frame) }
    }
}
