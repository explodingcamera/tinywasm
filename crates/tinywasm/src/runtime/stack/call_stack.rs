use crate::{runtime::RawWasmValue, BlockType, Error, LabelFrame, Result, Trap};
use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::{ValType, WasmValue};

use super::blocks::Labels;

// minimum call stack size
const CALL_STACK_SIZE: usize = 128;
const CALL_STACK_MAX_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
    top: usize,
}

impl Default for CallStack {
    fn default() -> Self {
        Self { stack: Vec::with_capacity(CALL_STACK_SIZE), top: 0 }
    }
}

impl CallStack {
    pub(crate) fn is_empty(&self) -> bool {
        self.top == 0
    }

    pub(crate) fn pop(&mut self) -> Result<CallFrame> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }

        self.top -= 1;
        Ok(self.stack.pop().unwrap())
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame) -> Result<()> {
        assert!(self.top <= self.stack.len(), "stack is too small");

        log::debug!("stack size: {}", self.stack.len());
        if self.stack.len() >= CALL_STACK_MAX_SIZE {
            return Err(Trap::CallStackOverflow.into());
        }

        self.top += 1;
        self.stack.push(call_frame);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
    pub(crate) func_ptr: usize,

    pub(crate) labels: Labels,
    pub(crate) locals: Box<[RawWasmValue]>,
    pub(crate) local_count: usize,
}

impl CallFrame {
    #[inline]
    /// Push a new label to the label stack and ensure the stack has the correct values
    pub(crate) fn enter_label(&mut self, label_frame: LabelFrame, stack: &mut super::ValueStack) {
        if label_frame.args.params > 0 {
            stack.extend_from_within((label_frame.stack_ptr - label_frame.args.params)..label_frame.stack_ptr);
        }

        self.labels.push(label_frame);
    }

    /// Break to a block at the given index (relative to the current frame)
    /// Returns `None` if there is no block at the given index (e.g. if we need to return, this is handled by the caller)
    #[inline]
    pub(crate) fn break_to(&mut self, break_to_relative: u32, value_stack: &mut super::ValueStack) -> Option<()> {
        log::debug!("break_to_relative: {}", break_to_relative);
        let break_to = self.labels.get_relative_to_top(break_to_relative as usize)?;

        // instr_ptr points to the label instruction, but the next step
        // will increment it by 1 since we're changing the "current" instr_ptr
        match break_to.ty {
            BlockType::Loop => {
                // this is a loop, so we want to jump back to the start of the loop
                // We also want to push the params to the stack
                value_stack.break_to(break_to.stack_ptr, break_to.args.params);

                self.instr_ptr = break_to.instr_ptr;

                // we also want to trim the label stack to the loop (but not including the loop)
                self.labels.truncate(self.labels.len() - break_to_relative as usize);
            }
            BlockType::Block | BlockType::If | BlockType::Else => {
                // this is a block, so we want to jump to the next instruction after the block ends
                // We also want to push the block's results to the stack
                value_stack.break_to(break_to.stack_ptr, break_to.args.results);

                // (the inst_ptr will be incremented by 1 before the next instruction is executed)
                self.instr_ptr = break_to.end_instr_ptr;

                // we also want to trim the label stack, including the block
                self.labels.truncate(self.labels.len() - (break_to_relative as usize + 1));
            }
        }

        Some(())
    }

    pub(crate) fn new_raw(func_ptr: usize, params: &[RawWasmValue], local_types: Vec<ValType>) -> Self {
        let mut locals = Vec::with_capacity(local_types.len() + params.len());
        locals.extend(params.iter().cloned());
        locals.extend(local_types.iter().map(|_| RawWasmValue::default()));

        Self {
            instr_ptr: 0,
            func_ptr,
            local_count: locals.len(),
            locals: locals.into_boxed_slice(),
            labels: Labels::default(),
        }
    }

    pub(crate) fn new(func_ptr: usize, params: &[WasmValue], local_types: Vec<ValType>) -> Self {
        CallFrame::new_raw(func_ptr, &params.iter().map(|v| RawWasmValue::from(*v)).collect::<Vec<_>>(), local_types)
    }

    #[inline]
    pub(crate) fn set_local(&mut self, local_index: usize, value: RawWasmValue) {
        assert!(local_index < self.local_count, "Invalid local index");
        self.locals[local_index] = value;
    }

    #[inline]
    pub(crate) fn get_local(&self, local_index: usize) -> RawWasmValue {
        assert!(local_index < self.local_count, "Invalid local index");
        self.locals[local_index]
    }
}
