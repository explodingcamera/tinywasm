use core::ops::ControlFlow;

use super::BlockType;
use crate::Trap;
use crate::interpreter::{Value128, values::*};
use crate::{Error, unlikely};

use alloc::boxed::Box;
use alloc::{rc::Rc, vec::Vec};
use tinywasm_types::{ArcSlice, Instruction, LocalAddr, ModuleInstanceAddr, WasmFunction, WasmFunctionData, WasmValue};

pub(crate) const MAX_CALL_STACK_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
}

impl CallStack {
    #[inline]
    pub(crate) fn new(config: &crate::engine::Config) -> Self {
        Self { stack: Vec::with_capacity(config.call_stack_init_size) }
    }

    pub(crate) fn clear(&mut self) {
        self.stack.clear();
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<CallFrame> {
        self.stack.pop()
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame) -> ControlFlow<Option<Error>> {
        if unlikely((self.stack.len() + 1) >= MAX_CALL_STACK_SIZE) {
            return ControlFlow::Break(Some(Trap::CallStackOverflow.into()));
        }
        self.stack.push(call_frame);
        ControlFlow::Continue(())
    }
}

#[derive(Debug)]
pub(crate) struct CallFrame {
    instr_ptr: usize,
    func_instance: Rc<WasmFunction>,
    block_ptr: u32,
    module_addr: ModuleInstanceAddr,
    pub(crate) locals: Locals,
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
    #[inline]
    pub(crate) fn instr_ptr(&self) -> usize {
        self.instr_ptr
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn data(&self) -> &WasmFunctionData {
        &self.func_instance.data
    }

    #[inline(always)]
    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }

    #[inline]
    pub(crate) fn jump(&mut self, offset: u32) {
        self.instr_ptr += offset as usize;
    }

    #[inline]
    pub(crate) fn module_addr(&self) -> ModuleInstanceAddr {
        self.module_addr
    }

    #[inline(always)]
    pub(crate) fn fetch_instr(&self) -> &Instruction {
        self
            .func_instance
            .instructions
            .get(self.instr_ptr)
            .unwrap_or_else(|| unreachable!("Instruction pointer out of bounds, this is a bug"))
    }

    #[inline]
    pub(crate) fn block_ptr(&self) -> u32 {
        self.block_ptr
    }

    pub(crate) fn reuse_for(
        &mut self,
        func: Rc<WasmFunction>,
        locals: Locals,
        block_depth: u32,
        module_addr: ModuleInstanceAddr,
    ) {
        self.func_instance = func;
        self.module_addr = module_addr;
        self.locals = locals;
        self.block_ptr = block_depth;
        self.instr_ptr = 0; // Reset to function entry
    }

    /// Break to a block at the given index (relative to the current frame)
    /// Returns `None` if there is no block at the given index (e.g. if we need to return, this is handled by the caller)
    #[inline]
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

    #[inline]
    pub(crate) fn new(
        func_instance: Rc<WasmFunction>,
        module_addr: ModuleInstanceAddr,
        params: &[WasmValue],
        block_ptr: u32,
    ) -> Self {
        let locals = {
            let mut locals_32 = Vec::with_capacity(func_instance.locals.c32 as usize);
            let mut locals_64 = Vec::with_capacity(func_instance.locals.c64 as usize);
            let mut locals_128 = Vec::with_capacity(func_instance.locals.c128 as usize);
            let mut locals_ref = Vec::with_capacity(func_instance.locals.cref as usize);

            for p in params {
                match p.into() {
                    TinyWasmValue::Value32(v) => locals_32.push(v),
                    TinyWasmValue::Value64(v) => locals_64.push(v),
                    TinyWasmValue::Value128(v) => locals_128.push(v),
                    TinyWasmValue::ValueRef(v) => locals_ref.push(v),
                }
            }

            locals_32.resize_with(func_instance.locals.c32 as usize, Default::default);
            locals_64.resize_with(func_instance.locals.c64 as usize, Default::default);
            locals_128.resize_with(func_instance.locals.c128 as usize, Default::default);
            locals_ref.resize_with(func_instance.locals.cref as usize, Default::default);

            Locals {
                locals_32: locals_32.into_boxed_slice(),
                locals_64: locals_64.into_boxed_slice(),
                locals_128: locals_128.into_boxed_slice(),
                locals_ref: locals_ref.into_boxed_slice(),
            }
        };

        Self { instr_ptr: 0, func_instance, module_addr, block_ptr, locals }
    }

    #[inline]
    pub(crate) fn new_raw(
        func_instance: Rc<WasmFunction>,
        module_addr: ModuleInstanceAddr,
        locals: Locals,
        block_ptr: u32,
    ) -> Self {
        Self { instr_ptr: 0, func_instance, module_addr, block_ptr, locals }
    }

    #[inline]
    pub(crate) fn instructions(&self) -> &ArcSlice<Instruction> {
        &self.func_instance.instructions
    }
}
