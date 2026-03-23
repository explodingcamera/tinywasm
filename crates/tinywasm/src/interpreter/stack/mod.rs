mod block_stack;
mod call_stack;
mod value_stack;

pub(crate) use block_stack::{BlockFrame, BlockStack, BlockType};
pub(crate) use call_stack::{CallFrame, CallStack, Locals};
pub(crate) use value_stack::ValueStack;

use crate::engine::Config;

/// A WebAssembly Stack
#[derive(Debug)]
pub(crate) struct Stack {
    pub(crate) values: ValueStack,
    pub(crate) blocks: BlockStack,
    pub(crate) call_stack: CallStack,
}

impl Stack {
    pub(crate) fn new(config: &Config) -> Self {
        Self { values: ValueStack::new(config), blocks: BlockStack::new(config), call_stack: CallStack::new(config) }
    }

    /// Initialize the stack with the given call frame (used for starting execution)
    pub(crate) fn initialize(&mut self, callframe: CallFrame) {
        self.blocks.clear();
        self.values.clear();
        self.call_stack.reset(callframe);
    }
}
