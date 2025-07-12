mod block_stack;
mod call_stack;
mod value_stack;

pub(crate) use block_stack::{BlockFrame, BlockStack, BlockType};
pub(crate) use call_stack::{CallFrame, CallStack, Locals};
pub(crate) use value_stack::ValueStack;

use crate::StackConfig;

/// A WebAssembly Stack
#[derive(Debug)]
pub(crate) struct Stack {
    pub(crate) values: ValueStack,
    pub(crate) blocks: BlockStack,
    pub(crate) call_stack: CallStack,
}

impl Stack {
    pub(crate) fn new(call_frame: CallFrame, config: &StackConfig) -> Self {
        Self {
            values: ValueStack::new(config),
            blocks: BlockStack::new(config),
            call_stack: CallStack::new(call_frame),
        }
    }
}
