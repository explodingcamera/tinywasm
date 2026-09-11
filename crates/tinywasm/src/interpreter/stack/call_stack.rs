use crate::{Result, Trap, interpreter::values::ValueRef};

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
    pub(crate) fn len(&self) -> u32 {
        self.stack.len() as u32
    }

    pub(crate) fn truncate_to(&mut self, len: u32) {
        debug_assert!(len as usize <= self.stack.len());
        self.stack.truncate(len as usize);
    }

    #[inline(always)]
    pub(crate) fn pop_frame(&mut self, base: u32) -> Option<CallFrame> {
        if self.len() == base { None } else { self.stack.pop() }
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, mut call_frame: CallFrame, instr_ptr: usize) -> Result<(), Trap> {
        if self.stack.len() == self.stack.capacity() && (!self.dynamic || self.stack.len() >= self.max_size) {
            return cold!(Err(Trap::CallStackOverflow));
        }

        call_frame.instr_ptr = instr_ptr;
        self.stack.push(call_frame);
        Ok(())
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
    pub(crate) func_addr: FuncAddr,
    pub(crate) locals_base: StackBase,
    pub(crate) stack_offset: ValueCounts,
    pub(crate) acc32: u32,
    pub(crate) acc64: u64,
    pub(crate) acc_ref: ValueRef,
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
        Self { instr_ptr: 0, func_addr, locals_base, stack_offset, acc32: 0, acc64: 0, acc_ref: ValueRef::NULL }
    }

    #[inline]
    pub(crate) fn stack_base(&self) -> StackBase {
        StackBase {
            s32: self.locals_base.s32 + self.stack_offset.c32 as u32,
            s64: self.locals_base.s64 + self.stack_offset.c64 as u32,
            s128: self.locals_base.s128 + self.stack_offset.c128 as u32,
        }
    }
}
