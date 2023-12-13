mod blocks;
mod call_stack;
mod value_stack;

use self::{call_stack::CallStack, value_stack::ValueStack};
pub(crate) use blocks::{BlockType, LabelFrame};
pub(crate) use call_stack::CallFrame;

/// A WebAssembly Stack
#[derive(Debug, Default)]
pub struct Stack {
    // keeping this typed for now to make it easier to debug
    // TODO: Maybe split into Vec<u8> and Vec<ValType> for better memory usage?
    pub(crate) values: ValueStack,

    /// The call stack
    pub(crate) call_stack: CallStack,
}
