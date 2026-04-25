use crate::{Result, Trap};
use core::hint::cold_path;

use alloc::vec::Vec;
use tinywasm_types::{FuncAddr, ValueCounts};

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
    max_size: usize,
    dynamic: bool,
}

impl CallStack {
    pub(crate) fn new(config: &crate::engine::Config) -> Self {
        let stack = config.call_stack;
        Self { stack: Vec::with_capacity(stack.initial_size), max_size: stack.max_size, dynamic: stack.dynamic }
    }

    pub(crate) fn clear(&mut self) {
        self.stack.clear();
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> Option<CallFrame> {
        self.stack.pop()
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, mut call_frame: CallFrame) -> Result<(), Trap> {
        self.ensure_capacity_for(self.stack.len() + 1)?;
        call_frame.incr_instr_ptr();
        self.stack.push(call_frame);
        Ok(())
    }

    #[inline(always)]
    fn ensure_capacity_for(&mut self, required_len: usize) -> Result<(), Trap> {
        if required_len <= self.stack.capacity() {
            return Ok(());
        }

        if required_len > self.max_size || !self.dynamic {
            cold_path();
            return Err(Trap::CallStackOverflow);
        }

        let target_capacity = required_len.max(self.stack.capacity().max(1).saturating_mul(2)).min(self.max_size);
        let Ok(()) = self.stack.try_reserve(target_capacity.saturating_sub(self.stack.len())) else {
            cold_path();
            return Err(Trap::CallStackOverflow);
        };
        Ok(())
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: u32,
    pub(crate) func_addr: FuncAddr,
    pub(crate) locals_base: StackBase,
    pub(crate) stack_offset: ValueCounts,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct StackBase {
    pub(crate) s32: u32,
    pub(crate) s64: u32,
    pub(crate) s128: u32,
}

impl CallFrame {
    pub(crate) fn new(func_addr: FuncAddr, locals_base: StackBase, stack_offset: ValueCounts) -> Self {
        Self { instr_ptr: 0, func_addr, locals_base, stack_offset }
    }

    #[inline]
    pub(crate) fn stack_base(&self) -> StackBase {
        StackBase {
            s32: self.locals_base.s32 + self.stack_offset.c32 as u32,
            s64: self.locals_base.s64 + self.stack_offset.c64 as u32,
            s128: self.locals_base.s128 + self.stack_offset.c128 as u32,
        }
    }

    #[inline(always)]
    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }
}
