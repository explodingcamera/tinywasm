mod call_stack;
mod value_stack;

pub(crate) use call_stack::{CallFrame, CallStack, Locals, StackBase};
pub(crate) use value_stack::ValueStack;

use crate::engine::Config;

/// A WebAssembly Stack
#[derive(Debug)]
pub(crate) struct Stack {
    pub(crate) values: ValueStack,
    pub(crate) call_stack: CallStack,
}

impl Stack {
    pub(crate) fn new(config: &Config) -> Self {
        Self { values: ValueStack::new(config), call_stack: CallStack::new(config) }
    }

    pub(crate) fn clear(&mut self) {
        self.values.clear();
        self.call_stack.clear();
    }
}
