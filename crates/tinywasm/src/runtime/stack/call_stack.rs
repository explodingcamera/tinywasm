use alloc::{boxed::Box, rc::Rc, vec::Vec};
use tinywasm_types::{ModuleInstanceAddr, WasmFunction};

use crate::runtime::{BlockType, RawWasmValue};
use crate::unlikely;
use crate::{Error, Result, Trap};

use super::BlockFrame;

const CALL_STACK_SIZE: usize = 128;
const CALL_STACK_MAX_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
}

impl CallStack {
    #[inline]
    pub(crate) fn new(initial_frame: CallFrame) -> Self {
        let mut stack = Self { stack: Vec::with_capacity(CALL_STACK_SIZE) };
        stack.push(initial_frame).unwrap();
        stack
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Result<CallFrame> {
        match self.stack.pop() {
            Some(frame) => Ok(frame),
            None => Err(Error::CallStackUnderflow),
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
    pub(crate) block_ptr: usize,
    pub(crate) func_instance: (Rc<WasmFunction>, ModuleInstanceAddr),
    pub(crate) locals: Box<[RawWasmValue]>,
}

impl CallFrame {
    /// Push a new label to the label stack and ensure the stack has the correct values
    pub(crate) fn enter_block(
        &mut self,
        block_frame: BlockFrame,
        values: &mut super::ValueStack,
        blocks: &mut super::BlockStack,
    ) {
        if block_frame.params > 0 {
            values.extend_from_within((block_frame.stack_ptr - block_frame.params)..block_frame.stack_ptr);
        }

        blocks.push(block_frame);
    }

    /// Break to a block at the given index (relative to the current frame)
    /// Returns `None` if there is no block at the given index (e.g. if we need to return, this is handled by the caller)
    pub(crate) fn break_to(
        &mut self,
        break_to_relative: u32,
        values: &mut super::ValueStack,
        blocks: &mut super::BlockStack,
    ) -> Option<()> {
        let break_to = blocks.get_relative_to(break_to_relative as usize, self.block_ptr)?;

        // instr_ptr points to the label instruction, but the next step
        // will increment it by 1 since we're changing the "current" instr_ptr
        match break_to.ty {
            BlockType::Loop => {
                // this is a loop, so we want to jump back to the start of the loop
                self.instr_ptr = break_to.instr_ptr;

                // We also want to push the params to the stack
                values.break_to(break_to.stack_ptr, break_to.params);

                // check if we're breaking to the loop
                if break_to_relative != 0 {
                    // we also want to trim the label stack to the loop (but not including the loop)
                    blocks.truncate(blocks.len() - break_to_relative as usize);
                    return Some(());
                }
            }

            BlockType::Block | BlockType::If | BlockType::Else => {
                // this is a block, so we want to jump to the next instruction after the block ends
                // We also want to push the block's results to the stack
                values.break_to(break_to.stack_ptr, break_to.results);

                // (the inst_ptr will be incremented by 1 before the next instruction is executed)
                self.instr_ptr = break_to.end_instr_ptr;

                // we also want to trim the label stack, including the block
                blocks.truncate(blocks.len() - (break_to_relative as usize + 1));
            }
        }

        Some(())
    }

    // TODO: perf: a lot of time is spent here
    #[inline(always)] // about 10% faster with this
    pub(crate) fn new(
        wasm_func_inst: Rc<WasmFunction>,
        owner: ModuleInstanceAddr,
        params: impl Iterator<Item = RawWasmValue> + ExactSizeIterator,
        block_ptr: usize,
    ) -> Self {
        let locals = {
            let local_types = &wasm_func_inst.locals;
            let total_size = local_types.len() + params.len();
            let mut locals = Vec::with_capacity(total_size);
            locals.extend(params);
            locals.resize_with(total_size, RawWasmValue::default);
            locals.into_boxed_slice()
        };

        Self { instr_ptr: 0, func_instance: (wasm_func_inst, owner), locals, block_ptr }
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
