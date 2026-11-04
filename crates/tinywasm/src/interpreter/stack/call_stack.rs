use crate::{Result, Trap};

use alloc::vec::Vec;
use tinywasm_types::{FuncAddr, ValueCounts};

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CallStack {
    stack: Vec<CallFrame>,
    max_size: usize,
    dynamic: bool,
}

impl CallStack {
    #[cfg(feature = "diagnostics")]
    pub(crate) fn debug_frames(&self) -> impl Iterator<Item = &CallFrame> {
        self.stack.iter().rev()
    }

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

    /// Reserves a frame before value-stack preparation changes any live values.
    #[inline]
    pub(crate) fn reserve_frame(&mut self) -> Result<(), Trap> {
        let Some(required) = self.stack.len().checked_add(1) else {
            return Err(Trap::CallStackOverflow);
        };
        if required > self.max_size || u32::try_from(required).is_err() {
            return cold!(Err(Trap::CallStackOverflow));
        }
        if required > self.stack.capacity() {
            core::hint::cold_path();
            if !self.dynamic {
                return Err(Trap::CallStackOverflow);
            }
            self.stack.try_reserve(1).map_err(|_| Trap::OutOfMemory)?;
        }
        Ok(())
    }

    /// Publishes a frame after its call-stack and value-stack reservations succeeded.
    #[inline(always)]
    pub(crate) fn push(&mut self, mut call_frame: CallFrame, instr_ptr: usize) {
        if self.stack.len() == self.stack.capacity() {
            crate::invariant_violated("call stack push without a reservation");
        }
        debug_assert!(self.stack.len() < self.max_size);
        call_frame.instr_ptr = instr_ptr;
        self.stack.push(call_frame);
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct CallFrame {
    pub(crate) instr_ptr: usize,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Config, StackConfig};

    #[test]
    fn spare_capacity_does_not_raise_the_limit() {
        let mut stack = CallStack::new(&Config::new().with_call_stack(StackConfig::dynamic(0, 1)));
        stack.stack.reserve(16);
        stack.reserve_frame().unwrap();
        stack.push(CallFrame::new(0, StackBase::default(), ValueCounts::default()), 7);
        assert!(matches!(stack.reserve_frame(), Err(Trap::CallStackOverflow)));
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.stack[0].instr_ptr, 7);
    }
}
