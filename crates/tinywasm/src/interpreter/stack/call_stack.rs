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
    pub(crate) locals: Locals,
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) func_addr: FuncAddr,
    pub(crate) stack_base: StackBase,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StackBase {
    pub(crate) s32: usize,
    pub(crate) s64: usize,
    pub(crate) s128: usize,
    pub(crate) sref: usize,
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
    pub(crate) fn new(
        func_addr: FuncAddr,
        module_addr: ModuleInstanceAddr,
        locals: Locals,
        stack_base: StackBase,
    ) -> Self {
        Self { instr_ptr: 0, func_addr, module_addr, locals, stack_base }
    }

    pub(crate) fn new_with_params(
        local_count: ValueCounts,
        func_addr: FuncAddr,
        module_addr: ModuleInstanceAddr,
        params: &[WasmValue],
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

        Self::new(func_addr, module_addr, locals, StackBase::default())
    }

    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }

    pub(crate) fn reuse_for(
        &mut self,
        func_addr: FuncAddr,
        locals: Locals,
        module_addr: ModuleInstanceAddr,
        stack_base: StackBase,
    ) {
        self.func_addr = func_addr;
        self.module_addr = module_addr;
        self.locals = locals;
        self.stack_base = stack_base;
        self.instr_ptr = 0;
    }
}
