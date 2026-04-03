use crate::{Result, Trap, unlikely};

use alloc::vec::Vec;
use tinywasm_types::{FuncAddr, ModuleInstanceAddr, ValueCountsSmall};

#[cfg_attr(feature = "debug", derive(Debug))]
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

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> Option<CallFrame> {
        self.stack.pop()
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, call_frame: CallFrame) -> Result<()> {
        if unlikely(self.stack.len() == self.stack.capacity()) {
            return Err(Trap::CallStackOverflow.into());
        }
        self.stack.push(call_frame);
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: u32,
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) func_addr: FuncAddr,
    pub(crate) locals_base: StackBase,
    pub(crate) stack_offset: ValueCountsSmall,
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct StackBase {
    pub(crate) s32: u32,
    pub(crate) s64: u32,
    pub(crate) s128: u32,
    pub(crate) sref: u32,
}

impl CallFrame {
    pub(crate) fn new(
        func_addr: FuncAddr,
        module_addr: ModuleInstanceAddr,
        locals_base: StackBase,
        stack_offset: ValueCountsSmall,
    ) -> Self {
        Self { instr_ptr: 0, func_addr, module_addr, locals_base, stack_offset }
    }

    #[inline]
    pub(crate) fn stack_base(&self) -> StackBase {
        StackBase {
            s32: self.locals_base.s32 + self.stack_offset.c32 as u32,
            s64: self.locals_base.s64 + self.stack_offset.c64 as u32,
            s128: self.locals_base.s128 + self.stack_offset.c128 as u32,
            sref: self.locals_base.sref + self.stack_offset.cref as u32,
        }
    }

    #[inline(always)]
    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }
}
