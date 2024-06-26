use super::values::TinyWasmValue;
use super::BlockType;
use crate::unlikely;
use crate::{Result, Trap};

use alloc::boxed::Box;
use alloc::{rc::Rc, vec, vec::Vec};
use tinywasm_types::{Instruction, LocalAddr, ModuleInstanceAddr, WasmFunction, WasmValue};

pub(crate) const MAX_CALL_STACK_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
}

impl CallStack {
    #[inline]
    pub(crate) fn new(initial_frame: CallFrame) -> Self {
        Self { stack: vec![initial_frame] }
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> Option<CallFrame> {
        self.stack.pop()
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, call_frame: CallFrame) -> Result<()> {
        if unlikely((self.stack.len() + 1) >= MAX_CALL_STACK_SIZE) {
            return Err(Trap::CallStackOverflow.into());
        }
        self.stack.push(call_frame);
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
    pub(crate) block_ptr: u32,
    pub(crate) func_instance: Rc<WasmFunction>,
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) locals: Box<[TinyWasmValue]>,
}

impl CallFrame {
    #[inline(always)]
    pub(crate) fn instr_ptr(&self) -> usize {
        self.instr_ptr
    }

    #[inline(always)]
    pub(crate) fn instr_ptr_mut(&mut self) -> &mut usize {
        &mut self.instr_ptr
    }

    #[inline(always)]
    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }

    #[inline(always)]
    pub(crate) fn module_addr(&self) -> ModuleInstanceAddr {
        self.module_addr
    }

    #[inline(always)]
    pub(crate) fn block_ptr(&self) -> u32 {
        self.block_ptr
    }

    #[inline(always)]
    pub(crate) fn fetch_instr(&self) -> &Instruction {
        match self.func_instance.instructions.get(self.instr_ptr) {
            Some(instr) => instr,
            None => unreachable!("Instruction pointer out of bounds"),
        }
    }

    /// Break to a block at the given index (relative to the current frame)
    /// Returns `None` if there is no block at the given index (e.g. if we need to return, this is handled by the caller)
    pub(crate) fn break_to(
        &mut self,
        break_to_relative: u32,
        values: &mut super::ValueStack,
        blocks: &mut super::BlockStack,
    ) -> Option<()> {
        let break_to = blocks.get_relative_to(break_to_relative, self.block_ptr)?;

        // instr_ptr points to the label instruction, but the next step
        // will increment it by 1 since we're changing the "current" instr_ptr
        match break_to.ty {
            BlockType::Loop => {
                // this is a loop, so we want to jump back to the start of the loop
                self.instr_ptr = break_to.instr_ptr;

                // We also want to push the params to the stack
                values.truncate_keep(&break_to.stack_ptr, &break_to.params);

                // check if we're breaking to the loop
                if break_to_relative != 0 {
                    // we also want to trim the label stack to the loop (but not including the loop)
                    blocks.truncate(blocks.len() as u32 - break_to_relative);
                    return Some(());
                }
            }

            BlockType::Block | BlockType::If | BlockType::Else => {
                // this is a block, so we want to jump to the next instruction after the block ends
                // We also want to push the block's results to the stack
                values.truncate_keep(&break_to.stack_ptr, &break_to.results);

                // (the inst_ptr will be incremented by 1 before the next instruction is executed)
                self.instr_ptr = break_to.instr_ptr + break_to.end_instr_offset as usize;

                // we also want to trim the label stack, including the block
                blocks.truncate(blocks.len() as u32 - (break_to_relative + 1));
            }
        }

        Some(())
    }

    #[inline(always)]
    pub(crate) fn new(
        wasm_func_inst: Rc<WasmFunction>,
        owner: ModuleInstanceAddr,
        params: &[WasmValue],
        block_ptr: u32,
    ) -> Self {
        Self::new_raw(wasm_func_inst, owner, params.iter().map(|v| v.into()), block_ptr)
    }

    #[inline(always)]
    pub(crate) fn new_raw(
        wasm_func_inst: Rc<WasmFunction>,
        owner: ModuleInstanceAddr,
        params: impl ExactSizeIterator<Item = TinyWasmValue>,
        block_ptr: u32,
    ) -> Self {
        let locals = {
            let total_size = wasm_func_inst.locals.len() + params.len();
            let mut locals = Vec::new();
            locals.reserve_exact(total_size);
            locals.extend(params);
            locals.resize_with(total_size, Default::default);
            locals.into_boxed_slice()
        };

        Self { instr_ptr: 0, func_instance: wasm_func_inst, module_addr: owner, block_ptr, locals }
    }

    #[inline(always)]
    pub(crate) fn set_local(&mut self, local_index: LocalAddr, value: TinyWasmValue) {
        self.locals[local_index as usize] = value;
    }

    #[inline(always)]
    pub(crate) fn get_local(&self, local_index: LocalAddr) -> TinyWasmValue {
        self.locals[local_index as usize]
    }

    #[inline(always)]
    pub(crate) fn instructions(&self) -> &[Instruction] {
        &self.func_instance.instructions
    }
}
