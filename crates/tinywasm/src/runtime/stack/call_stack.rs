use crate::{runtime::RawWasmValue, Error, Result};
use alloc::{boxed::Box, vec::Vec};
use log::info;
use tinywasm_types::{ValType, WasmValue};

use super::blocks::Blocks;

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

    pub(crate) block_frames: Blocks,
    pub(crate) locals: Box<[RawWasmValue]>,
    pub(crate) local_count: usize,
}

impl CallFrame {
    /// Break to a block at the given index (relative to the current frame)
    #[inline]
    pub(crate) fn break_to(&mut self, break_to_relative: u32, value_stack: &mut super::ValueStack) -> Result<()> {
        info!("we're in block_index: {}", self.block_frames.len());
        info!("block_frames: {:?}", self.block_frames);
        info!("break_to_relative: {}", break_to_relative);

        let block_frame = self
            .block_frames
            .get_relative_to_top(break_to_relative as usize)
            .ok_or(Error::BlockStackUnderflow)?;

        info!("so we're breaking to: {:?} ?", block_frame);

        todo!("break based on the type of the block we're breaking to");

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
            block_frames: Blocks::default(),
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
