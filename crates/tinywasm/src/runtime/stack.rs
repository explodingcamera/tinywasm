mod blocks;
mod call_stack;
mod value_stack;

use self::{call_stack::CallStack, value_stack::ValueStack};
pub(crate) use blocks::{BlockType, LabelArgs, LabelFrame};
pub(crate) use call_stack::CallFrame;

/// A WebAssembly Stack
#[derive(Debug, Default)]
pub struct Stack {
    pub(crate) values: ValueStack,
    pub(crate) call_stack: CallStack,
}
