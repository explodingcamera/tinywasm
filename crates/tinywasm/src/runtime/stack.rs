mod blocks;
mod call_stack;
mod value_stack;

use self::{call_stack::CallStack, value_stack::ValueStack};
pub(crate) use blocks::{BlockType, LabelFrame};
pub(crate) use call_stack::CallFrame;

/// A WebAssembly Stack
#[derive(Debug)]
pub struct Stack {
    pub(crate) values: ValueStack,
    pub(crate) call_stack: CallStack,
}

impl Stack {
    pub(crate) fn new(call_frame: CallFrame) -> Self {
        Self { values: ValueStack::default(), call_stack: CallStack::new(call_frame) }
    }
}
