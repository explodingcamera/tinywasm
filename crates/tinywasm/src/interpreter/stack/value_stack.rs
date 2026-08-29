use alloc::vec::Vec;
use tinywasm_types::{MemoryArch, ValueCounts};

use super::StackBase;
use crate::engine::{Config, StackConfig};
use crate::interpreter::*;
use crate::{Result, Trap};

#[cfg_attr(feature = "debug", derive(Debug))]
/// Physical value lanes used by the interpreter.
///
/// Guest values should normally be accessed through
/// [`InternalValue`] so their stack and global representation stays consistent.
pub(crate) struct ValueStack {
    pub(crate) stack_32: Stack<Value32>,
    pub(crate) stack_64: Stack<Value64>,
    pub(crate) stack_128: Stack<Value128>,
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct Stack<T: Copy + Default> {
    data: Vec<T>,
    max_size: usize,
    dynamic: bool,
}

impl<T: Copy + Default> Stack<T> {
    pub(crate) fn new(config: StackConfig) -> Self {
        Self { data: Vec::with_capacity(config.initial_size), max_size: config.max_size, dynamic: config.dynamic }
    }

    pub(crate) fn clear(&mut self) {
        self.data.clear();
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) -> Result<(), Trap> {
        if self.data.len() == self.data.capacity() && (!self.dynamic || self.data.len() >= self.max_size) {
            return cold!(Err(Trap::ValueStackOverflow));
        }
        self.data.push(value);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn push_copy(&mut self, index: usize) -> Result<(), Trap> {
        if self.data.len() == self.data.capacity() && (!self.dynamic || self.data.len() >= self.max_size) {
            return cold!(Err(Trap::ValueStackOverflow));
        }
        let value = self.data[index];
        self.data.push(value);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> T {
        match self.data.pop() {
            Some(value) => value,
            None => cold!(unreachable!("ValueStack underflow, this is a bug")),
        }
    }

    #[inline(always)]
    pub(crate) fn last(&self) -> &T {
        match self.data.last() {
            Some(value) => value,
            None => cold!(unreachable!("ValueStack underflow, this is a bug")),
        }
    }

    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> &T {
        &self.data[index]
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, index: usize, value: T) {
        self.data[index] = value;
    }

    #[inline(always)]
    pub(crate) fn copy(&mut self, from: usize, to: usize) {
        self.data[to] = self.data[from];
    }

    #[inline(always)]
    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        let len = self.data.len();
        debug_assert!(n <= len);
        if n >= len {
            return;
        }

        let keep = len.wrapping_sub(n).min(end_keep);
        self.data.copy_within(len.wrapping_sub(keep)..len, n);
        self.data.truncate(n.wrapping_add(keep));
    }

    #[inline(always)]
    pub(crate) fn truncate_to(&mut self, n: usize) {
        debug_assert!(n <= self.data.len());
        self.data.truncate(n);
    }

    #[inline(always)]
    pub(crate) fn truncate_to_one_tail(&mut self, n: usize) {
        debug_assert!(n < self.data.len());
        let last = self.pop();
        self.data.truncate(n);
        self.data.push(last);
    }

    #[inline]
    pub(crate) fn enter_locals(&mut self, param_count: usize, local_count: usize) -> Result<u32, Trap> {
        debug_assert!(param_count <= local_count);
        debug_assert!(param_count <= self.data.len());

        let len = self.data.len();
        let start = len - param_count;
        let end = start + local_count;

        if end > self.data.capacity() {
            core::hint::cold_path();
            if end > self.max_size || !self.dynamic {
                return Err(Trap::ValueStackOverflow);
            }
            let cap = self.data.capacity();
            let target = end.max(cap.max(1).saturating_mul(2)).min(self.max_size);
            if self.data.try_reserve(target - len).is_err() {
                return Err(Trap::ValueStackOverflow);
            }
        }

        self.data.resize(end, T::default());
        Ok(start as u32)
    }

    #[inline(always)]
    pub(crate) fn select_many(&mut self, count: usize, condition: bool) {
        if count == 0 {
            return;
        }

        let len = self.data.len();
        let needed = count.wrapping_mul(2);

        if len < needed {
            unreachable!("Stack underflow, this is a bug");
        }

        if !condition {
            let dst = len.wrapping_sub(needed);
            let src = len.wrapping_sub(count);
            self.data.copy_within(src..len, dst);
        }

        self.data.truncate(len.wrapping_sub(count));
    }
}

impl<'a, T: Copy + Default> IntoIterator for &'a Stack<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}
impl ValueStack {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            stack_32: Stack::new(config.value_stack_32),
            stack_64: Stack::new(config.value_stack_64),
            stack_128: Stack::new(config.value_stack_128),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.stack_32.clear();
        self.stack_64.clear();
        self.stack_128.clear();
    }

    #[inline(always)]
    pub(crate) fn base(&self) -> StackBase {
        StackBase {
            s32: self.stack_32.len() as u32,
            s64: self.stack_64.len() as u32,
            s128: self.stack_128.len() as u32,
        }
    }

    pub(crate) fn base_before(&self, counts: ValueCounts) -> StackBase {
        let base = self.base();
        StackBase {
            s32: base.s32 - counts.c32 as u32,
            s64: base.s64 - counts.c64 as u32,
            s128: base.s128 - counts.c128 as u32,
        }
    }

    #[inline(always)]
    pub(crate) fn pop_memory_operand(&mut self, arch: MemoryArch) -> Result<usize, Trap> {
        match arch {
            MemoryArch::I32 => Ok(u32::stack_pop(self) as usize),
            MemoryArch::I64 => {
                let value = u64::stack_pop(self);
                #[cfg(target_pointer_width = "64")]
                return Ok(value as usize);
                #[cfg(not(target_pointer_width = "64"))]
                return cold_err!(usize::try_from(value).map_err(|_| Trap::MemoryOutOfBounds {
                    offset: usize::MAX,
                    len: 0,
                    max: usize::MAX,
                }));
            }
        }
    }

    #[inline]
    pub(crate) fn select_multi(&mut self, counts: ValueCounts) {
        let condition = i32::stack_pop(self) != 0;
        self.stack_32.select_many(counts.c32 as usize, condition);
        self.stack_64.select_many(counts.c64 as usize, condition);
        self.stack_128.select_many(counts.c128 as usize, condition);
    }

    #[inline(always)]
    pub(crate) fn enter_locals(&mut self, params: &ValueCounts, locals: &ValueCounts) -> Result<StackBase, Trap> {
        let locals_base32 = self.stack_32.enter_locals(params.c32 as usize, locals.c32 as usize)?;
        let locals_base64 = self.stack_64.enter_locals(params.c64 as usize, locals.c64 as usize)?;
        let locals_base128 = self.stack_128.enter_locals(params.c128 as usize, locals.c128 as usize)?;
        Ok(StackBase { s32: locals_base32, s64: locals_base64, s128: locals_base128 })
    }

    #[inline(always)]
    pub(crate) fn truncate_keep_counts(&mut self, base: StackBase, keep: ValueCounts) {
        self.stack_32.truncate_keep(base.s32 as usize, keep.c32 as usize);
        self.stack_64.truncate_keep(base.s64 as usize, keep.c64 as usize);
        self.stack_128.truncate_keep(base.s128 as usize, keep.c128 as usize);
    }

    #[inline(always)]
    pub(crate) fn truncate_to_base(&mut self, base: StackBase) {
        self.stack_32.truncate_to(base.s32 as usize);
        self.stack_64.truncate_to(base.s64 as usize);
        self.stack_128.truncate_to(base.s128 as usize);
    }

    pub(crate) fn push_dyn(&mut self, value: RuntimeValue) -> Result<(), Trap> {
        match value {
            RuntimeValue::Value32(value) => Value32::stack_push(self, value),
            RuntimeValue::Value64(value) => Value64::stack_push(self, value),
            RuntimeValue::Value128(value) => Value128::stack_push(self, value),
            RuntimeValue::ValueRef(value) => ValueRef::stack_push(self, value),
        }
    }
}
