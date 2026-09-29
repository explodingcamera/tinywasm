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
/// One value lane: a stack of `len` values over `data`.
///
/// `data` holds every slot the stack has reached and only grows. The slots above `len` hold stale
/// values that are written before they are read again. A large operand-stack reservation stays
/// capacity, so its slots are written, and their pages touched, only when a push first reaches them.
/// Keeping the height outside the `Vec` lets the tail-call handlers carry it in a register and
/// write it back without `unsafe`.
pub(crate) struct Stack<T: Copy + Default> {
    data: Vec<T>,
    len: usize,
    max_size: usize,
    dynamic: bool,
}

/// The most slots [`Stack::enter_locals`] writes ahead of the pushes that reach them.
const WRITTEN_RESERVATION: usize = 64;

impl<T: Copy + Default> Stack<T> {
    pub(crate) fn new(config: StackConfig) -> Self {
        Self {
            data: Vec::with_capacity(config.initial_size),
            len: 0,
            max_size: config.max_size,
            dynamic: config.dynamic,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Pushes a value inside a function body. `enter_locals` reserved the function's whole operand
    /// stack, so the stack is never full here, and the handlers make no calls.
    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) {
        let len = self.len;
        match self.data.get_mut(len) {
            Some(slot) => *slot = value,
            None => self.push_first(value),
        }
        self.len = len + 1;
    }

    /// Adds the first slot at a height the stack has not reached before, within the reserved
    /// capacity: it never reallocates. On the tail-call build this makes no call.
    #[inline(always)]
    fn push_first(&mut self, value: T) {
        core::hint::cold_path();
        if self.data.len() == self.data.capacity() {
            crate::invariant_violated("value stack push beyond the function's reservation");
        }
        self.data.push(value);
    }

    /// Pushes a value outside a function body (host arguments and results), which no reservation
    /// covers, so a dynamic stack grows here if needed.
    #[inline(always)]
    pub(crate) fn push_or_grow(&mut self, value: T) -> Result<(), Trap> {
        let len = self.len;
        match self.data.get_mut(len) {
            Some(slot) => *slot = value,
            None => return self.push_grow(value),
        }
        self.len = len + 1;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn push_copy(&mut self, index: usize) {
        let value = *self.get(index);
        self.push(value);
    }

    #[cold]
    #[inline(never)]
    fn push_grow(&mut self, value: T) -> Result<(), Trap> {
        // Only the reserved capacity is free to use. Past it, check the limit, which Vec growth
        // may intentionally overshoot.
        if self.data.len() == self.data.capacity() && (!self.dynamic || self.data.len() >= self.max_size) {
            return Err(Trap::ValueStackOverflow);
        }
        self.data.push(value);
        self.len = self.data.len();
        Ok(())
    }

    /// Pops the top value. On an empty stack the index wraps around, so the bounds check in
    /// [`Self::get`] also catches an underflow.
    #[inline(always)]
    pub(crate) fn pop(&mut self) -> T {
        let index = self.len.wrapping_sub(1);
        let value = *self.get(index);
        self.len = index;
        value
    }

    #[inline(always)]
    pub(crate) fn last(&self) -> &T {
        self.get(self.len.wrapping_sub(1))
    }

    /// The slot at `index`, which validation keeps below the height. The check is against the
    /// slots in use at any height, which is what memory safety needs. Builds with debug assertions
    /// also check the height, and CI runs the tests optimized with them, with the release profile's
    /// wrapping arithmetic.
    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> &T {
        debug_assert!(index < self.len);
        match self.data.get(index) {
            Some(value) => value,
            None => crate::invariant_violated("value stack index out of range"),
        }
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, index: usize, value: T) {
        debug_assert!(index < self.len);
        match self.data.get_mut(index) {
            Some(slot) => *slot = value,
            None => crate::invariant_violated("value stack index out of range"),
        }
    }

    #[inline(always)]
    pub(crate) fn copy(&mut self, from: usize, to: usize) {
        let value = *self.get(from);
        self.set(to, value);
    }

    #[inline(always)]
    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        let len = self.len;
        debug_assert!(n <= len);
        if n >= len {
            return;
        }

        let keep = len.wrapping_sub(n).min(end_keep);
        if keep != 0 {
            // Copying to the start of the values above `n` leaves copy_within no check to fail.
            match self.data.get_mut(n..len) {
                Some(above) => above.copy_within(above.len().wrapping_sub(keep).., 0),
                None => crate::invariant_violated("value stack index out of range"),
            }
        }
        self.len = n.wrapping_add(keep);
    }

    #[inline(always)]
    pub(crate) fn truncate_to(&mut self, n: usize) {
        debug_assert!(n <= self.len);
        self.len = n;
    }

    #[inline(always)]
    pub(crate) fn truncate_to_one_tail(&mut self, n: usize) {
        debug_assert!(n < self.len);
        let last = self.pop();
        self.len = n;
        self.push(last);
    }

    /// Enters a function: turns its parameters into the first locals, zeroes the rest, and reserves
    /// room for its operand stack (`max_stack` values above the locals), so [`Self::push`] never
    /// has to grow the stack while the function runs.
    #[inline]
    pub(crate) fn enter_locals(
        &mut self,
        param_count: usize,
        local_count: usize,
        max_stack: usize,
    ) -> Result<u32, Trap> {
        debug_assert!(param_count <= local_count);
        debug_assert!(param_count <= self.len);

        let len = self.len;
        let start = len - param_count;
        let end = start + local_count;
        let reserve = end + max_stack;

        if reserve > self.data.len() {
            core::hint::cold_path();
            self.reserve_slots(end, reserve)?;
        }

        // Most functions have no or few locals in a lane. Store the first and last directly and
        // fill only what lies between: `fill` becomes a memset call even for one value.
        if end > len {
            match self.data.get_mut(len..end) {
                Some([a]) => *a = T::default(),
                Some([a, middle @ .., b]) => {
                    if !middle.is_empty() {
                        middle.fill(T::default());
                    }
                    (*a, *b) = (T::default(), T::default());
                }
                _ => crate::invariant_violated("value stack index out of range"),
            }
        }
        self.len = end;
        Ok(start as u32)
    }

    /// Makes slots for a function's locals and reserves its operand stack when they reach past the
    /// slots the stack has had. A reservation of up to [`WRITTEN_RESERVATION`] slots is written out
    /// with the locals, so later entries at this height skip this. A larger one stays capacity and
    /// its slots are written as they are pushed, so a function whose deep branch rarely runs does
    /// not keep pages for it; its entries come back here.
    #[cold]
    #[inline(never)]
    fn reserve_slots(&mut self, end: usize, reserve: usize) -> Result<(), Trap> {
        if reserve > self.data.capacity() {
            self.grow_to(reserve)?;
        }
        let slots = if reserve - self.data.len() <= WRITTEN_RESERVATION { reserve } else { end };
        if slots > self.data.len() {
            self.data.resize(slots, T::default());
        }
        Ok(())
    }

    /// Makes room for `reserve` slots, growing the allocation if a dynamic stack allows it.
    #[cold]
    #[inline(never)]
    fn grow_to(&mut self, reserve: usize) -> Result<(), Trap> {
        if reserve > self.max_size || !self.dynamic {
            return Err(Trap::ValueStackOverflow);
        }
        let target = reserve.max(self.data.capacity().max(1).saturating_mul(2)).min(self.max_size);
        if self.data.try_reserve(target - self.data.len()).is_err() {
            return Err(Trap::ValueStackOverflow);
        }
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn select_many(&mut self, count: usize, condition: bool) {
        if count == 0 {
            return;
        }

        let len = self.len;
        let needed = count.wrapping_mul(2);

        if len < needed {
            crate::invariant_violated("value stack underflow");
        }

        if !condition {
            let dst = len.wrapping_sub(needed);
            let src = len.wrapping_sub(count);
            match self.data.get_mut(..len) {
                Some(live) => live.copy_within(src..len, dst),
                None => crate::invariant_violated("value stack index out of range"),
            }
        }

        self.len = len.wrapping_sub(count);
    }
}

impl<'a, T: Copy + Default> IntoIterator for &'a Stack<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data[..self.len].iter()
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
    pub(crate) fn enter_locals(
        &mut self,
        params: &ValueCounts,
        locals: &ValueCounts,
        max_stack: &ValueCounts,
    ) -> Result<StackBase, Trap> {
        let locals_base32 =
            self.stack_32.enter_locals(params.c32 as usize, locals.c32 as usize, max_stack.c32 as usize)?;
        let locals_base64 =
            self.stack_64.enter_locals(params.c64 as usize, locals.c64 as usize, max_stack.c64 as usize)?;
        let locals_base128 =
            self.stack_128.enter_locals(params.c128 as usize, locals.c128 as usize, max_stack.c128 as usize)?;
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

    /// Pushes a dynamically typed value inside a function body using its entry reservation.
    pub(crate) fn push_reserved(&mut self, value: RuntimeValue) {
        match value {
            RuntimeValue::Value32(value) => self.stack_32.push(value),
            RuntimeValue::Value64(value) => self.stack_64.push(value),
            RuntimeValue::Value128(value) => self.stack_128.push(value),
            RuntimeValue::ValueRef(value) => self.stack_32.push(value.raw()),
        }
    }

    /// Pushes a value from outside a function body's reservation; see [`Stack::push_or_grow`].
    pub(crate) fn push_dyn(&mut self, value: RuntimeValue) -> Result<(), Trap> {
        match value {
            RuntimeValue::Value32(value) => self.stack_32.push_or_grow(value),
            RuntimeValue::Value64(value) => self.stack_64.push_or_grow(value),
            RuntimeValue::Value128(value) => self.stack_128.push_or_grow(value),
            RuntimeValue::ValueRef(value) => self.stack_32.push_or_grow(value.raw()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stack of height 2 whose third slot still holds a popped value.
    #[cfg(debug_assertions)]
    fn popped() -> Stack<Value32> {
        let mut stack = Stack::new(StackConfig::fixed(8));
        for value in [1, 2, 3] {
            stack.push_or_grow(value).unwrap();
        }
        assert_eq!(stack.pop(), 3);
        stack
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn get_above_the_height() {
        popped().get(2);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic]
    fn set_above_the_height() {
        popped().set(2, 0);
    }

    /// A large operand-stack reservation is not written: only the locals and the values pushed
    /// become slots. A small one is written with the locals.
    #[test]
    fn large_reservations_are_written_as_they_are_pushed() {
        let mut stack = Stack::<Value64>::new(StackConfig::fixed(1024));
        assert!(matches!(stack.enter_locals(0, 2, 500), Ok(0)));
        assert_eq!(stack.data.len(), 2);
        for value in 0..500 {
            stack.push(value);
        }
        assert_eq!((stack.len(), stack.data.len()), (502, 502));

        // Below the slots the stack already has, entering a function writes only its locals.
        stack.truncate_to(2);
        stack.push(7);
        assert!(matches!(stack.enter_locals(0, 2, 10), Ok(3)));
        assert_eq!((stack.len(), stack.data.len()), (5, 502));

        stack.truncate_to(0);
        for value in 0..500 {
            stack.push_or_grow(value).unwrap();
        }
        assert!(matches!(stack.enter_locals(0, 1, 8), Ok(500)));
        assert_eq!(stack.data.len(), 509);
        assert!(matches!(stack.enter_locals(0, 2, 1000), Err(Trap::ValueStackOverflow)));
    }
}
