use super::BlockType;
use crate::interpreter::{Value128, values::*};
use crate::{Result, Trap, unlikely};

use alloc::boxed::Box;
use alloc::vec::Vec;
use tinywasm_types::{FuncAddr, LocalAddr, ModuleInstanceAddr, ValueCounts, WasmValue};

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
}

impl CallStack {
    pub(crate) fn new(config: &crate::engine::Config) -> Self {
        Self { stack: Vec::with_capacity(config.call_stack_size) }
    }

    pub(crate) fn clear(&mut self) {
        self.stack.clear();
    }

    pub(crate) fn pop(&mut self) -> Option<CallFrame> {
        self.stack.pop()
    }

    pub(crate) fn push(&mut self, call_frame: CallFrame) -> Result<()> {
        if unlikely(self.stack.len() >= self.stack.capacity()) {
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
    pub(crate) locals: Locals,
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) func_addr: FuncAddr,
}

#[derive(Debug)]
pub(crate) struct Locals {
    pub(crate) locals_32: Box<[Value32]>,
    pub(crate) locals_64: Box<[Value64]>,
    pub(crate) locals_128: Box<[Value128]>,
    pub(crate) locals_ref: Box<[ValueRef]>,
}

impl Locals {
    pub(crate) fn get<T: InternalValue>(&self, local_index: LocalAddr) -> T {
        T::local_get(self, local_index)
    }

    pub(crate) fn set<T: InternalValue>(&mut self, local_index: LocalAddr, value: T) {
        T::local_set(self, local_index, value);
    }
}

impl CallFrame {
    pub(crate) fn new(func_addr: FuncAddr, module_addr: ModuleInstanceAddr, locals: Locals, block_ptr: u32) -> Self {
        Self { instr_ptr: 0, func_addr, module_addr, block_ptr, locals }
    }

    pub(crate) fn new_with_params(
        local_count: ValueCounts,
        func_addr: FuncAddr,
        module_addr: ModuleInstanceAddr,
        params: &[WasmValue],
        block_ptr: u32,
    ) -> Self {
        let locals = {
            let mut locals_32 = Vec::with_capacity(local_count.c32 as usize);
            let mut locals_64 = Vec::with_capacity(local_count.c64 as usize);
            let mut locals_128 = Vec::with_capacity(local_count.c128 as usize);
            let mut locals_ref = Vec::with_capacity(local_count.cref as usize);

            for p in params {
                match p.into() {
                    TinyWasmValue::Value32(v) => locals_32.push(v),
                    TinyWasmValue::Value64(v) => locals_64.push(v),
                    TinyWasmValue::Value128(v) => locals_128.push(v),
                    TinyWasmValue::ValueRef(v) => locals_ref.push(v),
                }
            }

            locals_32.resize_with(local_count.c32 as usize, Default::default);
            locals_64.resize_with(local_count.c64 as usize, Default::default);
            locals_128.resize_with(local_count.c128 as usize, Default::default);
            locals_ref.resize_with(local_count.cref as usize, Default::default);

            Locals {
                locals_32: locals_32.into_boxed_slice(),
                locals_64: locals_64.into_boxed_slice(),
                locals_128: locals_128.into_boxed_slice(),
                locals_ref: locals_ref.into_boxed_slice(),
            }
        };

        Self::new(func_addr, module_addr, locals, block_ptr)
    }

    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }

    pub(crate) fn jump(&mut self, offset: u32) {
        self.instr_ptr += offset as usize;
    }

    pub(crate) fn reuse_for(
        &mut self,
        func_addr: FuncAddr,
        locals: Locals,
        block_depth: u32,
        module_addr: ModuleInstanceAddr,
    ) {
        self.func_addr = func_addr;
        self.module_addr = module_addr;
        self.locals = locals;
        self.block_ptr = block_depth;
        self.instr_ptr = 0; // Reset to function entry
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
                self.instr_ptr = break_to.instr_ptr as usize;

                // We also want to push the params to the stack
                values.truncate_keep(break_to.stack_ptr, break_to.params);

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
                values.truncate_keep(break_to.stack_ptr, break_to.results);

                // (the inst_ptr will be incremented by 1 before the next instruction is executed)
                self.instr_ptr = (break_to.instr_ptr + break_to.end_instr_offset) as usize;

                // we also want to trim the label stack, including the block
                blocks.truncate(blocks.len() as u32 - (break_to_relative + 1));
            }
        }

        Some(())
    }
}
