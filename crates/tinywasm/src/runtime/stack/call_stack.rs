use crate::{runtime::RawWasmValue, BlockType, Error, LabelFrame, Result};
use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::{ValType, WasmValue};

use super::blocks::Labels;

// minimum call stack size
const CALL_STACK_SIZE: usize = 1024;
const CALL_STACK_MAX_SIZE: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
    top: usize,
}

impl Default for CallStack {
    fn default() -> Self {
        Self {
            stack: Vec::with_capacity(CALL_STACK_SIZE),
            top: 0,
        }
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
    pub(crate) fn _top(&self) -> Result<&CallFrame> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn _top_mut(&mut self) -> Result<&mut CallFrame> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&mut self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame) {
        assert!(self.top <= self.stack.len());
        assert!(self.stack.len() <= CALL_STACK_MAX_SIZE);

        self.top += 1;
        self.stack.push(call_frame);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    // having real pointers here would be nice :( but we can't really do that in safe rust
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
    #[inline]
    pub(crate) fn break_to(&mut self, break_to_relative: u32, value_stack: &mut super::ValueStack) -> Result<()> {
        let current_label = self.labels.top().ok_or(Error::LabelStackUnderflow)?;
        let break_to = self
            .labels
            .get_relative_to_top(break_to_relative as usize)
            .ok_or(Error::LabelStackUnderflow)?;

        value_stack.break_to(break_to.stack_ptr, break_to.args.results as usize);

        // instr_ptr points to the label instruction, but the next step
        // will increment it by 1 since we're changing the "current" instr_ptr
        match break_to.ty {
            BlockType::Loop => {
                // this is a loop, so we want to jump back to the start of the loop
                self.instr_ptr = break_to.instr_ptr;

                // we also want to trim the label stack to the loop (but not including the loop)
                self.labels.truncate(self.labels.len() - break_to_relative as usize);
            }
            BlockType::Block => {
                // this is a block, so we want to jump to the next instruction after the block ends (the inst_ptr will be incremented by 1 before the next instruction is executed)
                self.instr_ptr = break_to.end_instr_ptr;

                // we also want to trim the label stack, including the block
                self.labels
                    .truncate(self.labels.len() - (break_to_relative as usize + 1));
            }
            _ => unimplemented!("break to block type: {:?}", current_label.ty),
        }

        // self.instr_ptr = block_frame.instr_ptr;
        // value_stack.trim(block_frame.stack_ptr);

        // // // Adjusting how to trim the blocks stack based on the block type
        // // let trim_index = match block_frame.block {
        // //     // if we are breaking to a loop, we want to jump back to the start of the loop
        // //     BlockFrameInner::Loop => block_index as usize - 1,
        // //     // if we are breaking to any other block, we want to jump to the end of the block
        // //     // TODO: check if this is correct
        // //     BlockFrameInner::If | BlockFrameInner::Else | BlockFrameInner::Block => block_index as usize - 1,
        // // };

        // self.block_frames.trim(block_index as usize);
        Ok(())
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
        CallFrame::new_raw(
            func_ptr,
            &params.iter().map(|v| RawWasmValue::from(*v)).collect::<Vec<_>>(),
            local_types,
        )
    }

    #[inline]
    pub(crate) fn set_local(&mut self, local_index: usize, value: RawWasmValue) {
        if local_index >= self.local_count {
            panic!("Invalid local index");
        }

        self.locals[local_index] = value;
    }

    #[inline]
    pub(crate) fn get_local(&self, local_index: usize) -> RawWasmValue {
        if local_index >= self.local_count {
            panic!("Invalid local index");
        }

        self.locals[local_index]
    }
}
