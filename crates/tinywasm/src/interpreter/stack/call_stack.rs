use core::ops::ControlFlow;

use super::BlockType;
use crate::interpreter::values::*;
use crate::Trap;
use crate::{unlikely, Error};

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
    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }

    #[inline]
    pub(crate) fn jump(&mut self, offset: usize) {
        self.instr_ptr += offset;
    }

    #[inline]
    pub(crate) fn module_addr(&self) -> ModuleInstanceAddr {
        self.module_addr
    }

    #[inline]
    pub(crate) fn block_ptr(&self) -> u32 {
        self.block_ptr
    }

    #[inline(always)]
    pub(crate) fn fetch_instr(&self) -> &Instruction {
        match self.func_instance.instructions.get(self.instr_ptr) {
            Some(instr) => instr,
            None => unreachable!("Instruction out of bounds, this is a bug"),
        }
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
                self.instr_ptr = break_to.instr_ptr;

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
                self.instr_ptr = break_to.instr_ptr + break_to.end_instr_offset as usize;

                // we also want to trim the label stack, including the block
                blocks.truncate(blocks.len() as u32 - (break_to_relative + 1));
            }
        }

        Some(())
    }

    #[inline]
    pub(crate) fn new(
        wasm_func_inst: Rc<WasmFunction>,
        owner: ModuleInstanceAddr,
        params: &[WasmValue],
        block_ptr: u32,
    ) -> Self {
        let locals = {
            let mut locals_32 = Vec::new();
            locals_32.reserve_exact(wasm_func_inst.locals.c32 as usize);
            let mut locals_64 = Vec::new();
            locals_64.reserve_exact(wasm_func_inst.locals.c64 as usize);
            let mut locals_128 = Vec::new();
            locals_128.reserve_exact(wasm_func_inst.locals.c128 as usize);
            let mut locals_ref = Vec::new();
            locals_ref.reserve_exact(wasm_func_inst.locals.cref as usize);

            for p in params {
                match p.into() {
                    TinyWasmValue::Value32(v) => locals_32.push(v),
                    TinyWasmValue::Value64(v) => locals_64.push(v),
                    TinyWasmValue::Value128(v) => locals_128.push(v),
                    TinyWasmValue::ValueRef(v) => locals_ref.push(v),
                }
            }

            locals_32.resize_with(wasm_func_inst.locals.c32 as usize, Default::default);
            locals_64.resize_with(wasm_func_inst.locals.c64 as usize, Default::default);
            locals_128.resize_with(wasm_func_inst.locals.c128 as usize, Default::default);
            locals_ref.resize_with(wasm_func_inst.locals.cref as usize, Default::default);

            Locals {
                locals_32: locals_32.into_boxed_slice(),
                locals_64: locals_64.into_boxed_slice(),
                locals_128: locals_128.into_boxed_slice(),
                locals_ref: locals_ref.into_boxed_slice(),
            }
        };

        Self { instr_ptr: 0, func_instance: wasm_func_inst, module_addr: owner, block_ptr, locals }
    }

    #[inline]
    pub(crate) fn new_raw(
        wasm_func_inst: Rc<WasmFunction>,
        owner: ModuleInstanceAddr,
        locals: Locals,
        block_ptr: u32,
    ) -> Self {
        Self { instr_ptr: 0, func_instance: wasm_func_inst, module_addr: owner, block_ptr, locals }
    }

    #[inline]
    pub(crate) fn instructions(&self) -> &[Instruction] {
        &self.func_instance.instructions
    }
}
