use crate::{Result, Trap, unlikely};

use alloc::vec::Vec;
use tinywasm_types::{FuncAddr, ModuleInstanceAddr, ValueCountsSmall};

#[derive(Debug)]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
    len: usize,
}

impl CallStack {
    pub(crate) fn new(config: &crate::engine::Config) -> Self {
        let mut stack = Vec::with_capacity(config.call_stack_size);
        stack.resize_with(config.call_stack_size, CallFrame::default);
        Self { stack, len: 0 }
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn pop(&mut self) -> Option<CallFrame> {
        if self.len == 0 {
            return None;
        }

        self.len -= 1;
        Some(self.stack[self.len])
    }

    pub(crate) fn is_full(&self) -> bool {
        self.len >= self.stack.len()
    }

    pub(crate) fn push(&mut self, call_frame: CallFrame) -> Result<()> {
        if unlikely(self.is_full()) {
            return Err(Trap::CallStackOverflow.into());
        }

        self.stack[self.len] = call_frame;
        self.len += 1;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
    pub(crate) module_addr: ModuleInstanceAddr,
    pub(crate) func_addr: FuncAddr,
    pub(crate) locals_base: StackBase,
    pub(crate) stack_offset: ValueCountsSmall,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StackBase {
    pub(crate) s32: usize,
    pub(crate) s64: usize,
    pub(crate) s128: usize,
    pub(crate) sref: usize,
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
            s32: self.locals_base.s32 + self.stack_offset.c32 as usize,
            s64: self.locals_base.s64 + self.stack_offset.c64 as usize,
            s128: self.locals_base.s128 + self.stack_offset.c128 as usize,
            sref: self.locals_base.sref + self.stack_offset.cref as usize,
        }
    }

    pub(crate) fn incr_instr_ptr(&mut self) {
        self.instr_ptr += 1;
    }
}
