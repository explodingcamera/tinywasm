use crate::{runtime::RawWasmValue, Error, Result};
use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::{ValType, WasmValue};

use super::{blocks::Blocks, BlockFrameType};

// minimum call stack size
const CALL_STACK_SIZE: usize = 1024;

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
    #[inline]
    pub(crate) fn _top(&self) -> Result<&CallFrame> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn top_mut(&mut self) -> Result<&mut CallFrame> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&mut self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame) {
        self.top += 1;
        self.stack.push(call_frame);
    }
}

#[derive(Debug)]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
    pub(crate) _func_ptr: usize,

    pub(crate) blocks: Blocks,
    pub(crate) locals: Box<[RawWasmValue]>,
    pub(crate) local_count: usize,
}

impl CallFrame {
    /// Break to a block at the given index (relative to the current frame)
    #[inline]
    pub(crate) fn break_to(&mut self, block_index: u32, value_stack: &mut super::ValueStack) -> Result<()> {
        let block = self
            .blocks
            .get(block_index as usize)
            .ok_or(Error::BlockStackUnderflow)?;

        self.instr_ptr = block.instr_ptr;
        value_stack.trim(block.stack_ptr);

        // Adjusting how to trim the blocks stack based on the block type
        let trim_index = match block.ty {
            // if we are breaking to a loop, we want to jump back to the start of the loop
            BlockFrameType::Loop => block_index as usize - 2,
            // if we are breaking to any other block, we want to jump to the end of the block
            // TODO: check if this is correct
            BlockFrameType::If | BlockFrameType::Else | BlockFrameType::Block => block_index as usize - 1,
        };

        self.blocks.trim(trim_index);
        Ok(())
    }

    pub(crate) fn new(func_ptr: usize, params: &[WasmValue], local_types: Vec<ValType>) -> Self {
        let mut locals = Vec::with_capacity(local_types.len() + params.len());
        locals.extend(params.iter().map(|v| RawWasmValue::from(*v)));
        locals.extend(local_types.iter().map(|_| RawWasmValue::default()));

        Self {
            instr_ptr: 0,
            _func_ptr: func_ptr,
            local_count: locals.len(),
            locals: locals.into_boxed_slice(),
            blocks: Blocks::default(),
        }
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
