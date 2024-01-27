use crate::unlikely;
use crate::{
    runtime::{BlockType, RawWasmValue},
    Error, FunctionInstance, Result, Trap,
};
use alloc::{boxed::Box, rc::Rc, vec::Vec};
use tinywasm_types::ValType;

use super::{blocks::Labels, LabelFrame};

// minimum call stack size
const CALL_STACK_SIZE: usize = 128;
const CALL_STACK_MAX_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
}

impl Default for CallStack {
    fn default() -> Self {
        Self { stack: Vec::with_capacity(CALL_STACK_SIZE) }
    }
}

impl CallStack {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Result<CallFrame> {
        match self.stack.pop() {
            Some(frame) => Ok(frame),
            None => Err(Error::CallStackEmpty),
        }
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame) -> Result<()> {
        if unlikely(self.stack.len() >= CALL_STACK_MAX_SIZE) {
            return Err(Trap::CallStackOverflow.into());
        }
        self.stack.push(call_frame);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
    // pub(crate) module: ModuleInstanceAddr,
    pub(crate) func_instance: Rc<FunctionInstance>,

    pub(crate) labels: Labels,
    pub(crate) locals: Box<[RawWasmValue]>,
}

impl CallFrame {
    #[inline]
    /// Push a new label to the label stack and ensure the stack has the correct values
    pub(crate) fn enter_label(&mut self, label_frame: LabelFrame, stack: &mut super::ValueStack) {
        if label_frame.params > 0 {
            stack.extend_from_within((label_frame.stack_ptr - label_frame.params)..label_frame.stack_ptr);
        }

        self.labels.push(label_frame);
    }

    /// Break to a block at the given index (relative to the current frame)
    /// Returns `None` if there is no block at the given index (e.g. if we need to return, this is handled by the caller)
    pub(crate) fn break_to(&mut self, break_to_relative: u32, value_stack: &mut super::ValueStack) -> Option<()> {
        let break_to = self.labels.get_relative_to_top(break_to_relative as usize)?;

        // instr_ptr points to the label instruction, but the next step
        // will increment it by 1 since we're changing the "current" instr_ptr
        match break_to.ty {
            BlockType::Loop => {
                // this is a loop, so we want to jump back to the start of the loop
                // We also want to push the params to the stack
                value_stack.break_to(break_to.stack_ptr, break_to.params);

                self.instr_ptr = break_to.instr_ptr;

                // we also want to trim the label stack to the loop (but not including the loop)
                self.labels.truncate(self.labels.len() - break_to_relative as usize);
            }
            BlockType::Block | BlockType::If | BlockType::Else => {
                // this is a block, so we want to jump to the next instruction after the block ends
                // We also want to push the block's results to the stack
                value_stack.break_to(break_to.stack_ptr, break_to.results);

                // (the inst_ptr will be incremented by 1 before the next instruction is executed)
                self.instr_ptr = break_to.end_instr_ptr;

                // we also want to trim the label stack, including the block
                self.labels.truncate(self.labels.len() - (break_to_relative as usize + 1));
            }
        }

        Some(())
    }

    #[inline]
    pub(crate) fn new(
        func_instance_ptr: Rc<FunctionInstance>,
        params: impl Iterator<Item = RawWasmValue> + ExactSizeIterator,
        local_types: Vec<ValType>,
    ) -> Self {
        let locals = {
            let total_size = local_types.len() + params.len();
            let mut locals = Vec::with_capacity(total_size);
            locals.extend(params);
            locals.resize_with(total_size, RawWasmValue::default);
            locals.into_boxed_slice()
        };

        Self { instr_ptr: 0, func_instance: func_instance_ptr, locals, labels: Labels::new() }
    }

    #[inline]
    pub(crate) fn set_local(&mut self, local_index: usize, value: RawWasmValue) {
        self.locals[local_index] = value;
    }

    #[inline]
    pub(crate) fn get_local(&self, local_index: usize) -> RawWasmValue {
        self.locals[local_index]
    }
}
