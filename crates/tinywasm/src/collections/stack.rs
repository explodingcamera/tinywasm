use alloc::vec::Vec;

use crate::Trap;
use crate::engine::StackConfig;

const MIN_DYNAMIC_CAPACITY: usize = 4;

/// One operand lane with initialized storage and a separate live height.
///
/// Only `slots[..top]` contains live values. Slots above it may contain popped values and must not
/// be read as locals or traced as GC roots. Ordinary pushes use storage reserved at function entry.
#[derive(Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct StackLane<T: Copy + Default> {
    slots: Vec<T>,
    top: usize,
    max_size: usize,
    dynamic: bool,
}

impl<T: Copy + Default> StackLane<T> {
    /// Reserves the configured starting capacity without initializing unused storage.
    pub(crate) fn new(config: StackConfig) -> Self {
        let size = config.initial_size.min(config.max_size).min(u32::MAX as usize);
        Self { slots: Vec::with_capacity(size), top: 0, max_size: config.max_size, dynamic: config.dynamic }
    }

    /// Removes live values while retaining initialized storage.
    pub(crate) fn clear(&mut self) {
        self.top = 0;
    }

    /// Returns the number of live values.
    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.top
    }

    /// Returns live values without exposing popped or uninitialized storage.
    pub(crate) fn as_slice(&self) -> &[T] {
        &self.slots[..self.top]
    }

    /// Synchronizes the unchanged live height forwarded into a tail-dispatch handler.
    #[cfg(tw_tail_calls)]
    #[inline(always)]
    pub(crate) fn set_height(&mut self, height: usize) {
        debug_assert_eq!(height, self.top);
        self.top = height;
    }

    /// Pushes a value within the current function's reservation, without growing the allocation.
    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) {
        if let Some(slot) = self.slots.get_mut(self.top) {
            *slot = value;
        } else {
            if self.top != self.slots.len() || self.slots.len() == self.slots.capacity() {
                crate::invariant_violated("value stack push beyond the function's reservation");
            }
            self.slots.push(value);
        }
        self.top += 1;
    }

    /// Pushes a host argument or result, growing fallibly if its reservation requires it.
    #[inline(always)]
    pub(crate) fn push_or_grow(&mut self, value: T) -> Result<(), Trap> {
        let Some(required) = self.top.checked_add(1) else {
            return Err(Trap::ValueStackOverflow);
        };
        self.reserve(required)?;
        self.push(value);
        Ok(())
    }

    /// Pushes a copy of a live value.
    #[inline(always)]
    pub(crate) fn push_copy(&mut self, index: usize) {
        let value = *self.get(index);
        self.push(value);
    }

    #[inline]
    fn reserve(&mut self, required: usize) -> Result<(), Trap> {
        if required > self.max_size || u32::try_from(required).is_err() {
            return cold!(Err(Trap::ValueStackOverflow));
        }
        if required > self.slots.capacity() {
            core::hint::cold_path();
            self.grow(required)?;
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn grow(&mut self, required: usize) -> Result<(), Trap> {
        if !self.dynamic {
            return Err(Trap::ValueStackOverflow);
        }
        let limit = self.max_size.min(u32::MAX as usize);
        let capacity = self.slots.capacity().min(limit);
        let target = required.max(capacity.checked_mul(2).unwrap_or(limit)).max(MIN_DYNAMIC_CAPACITY).min(limit);
        self.slots.try_reserve_exact(target - self.slots.len()).map_err(|_| Trap::OutOfMemory)?;
        Ok(())
    }

    /// Removes and returns the last live value.
    #[inline(always)]
    pub(crate) fn pop(&mut self) -> T {
        let Some(top) = self.top.checked_sub(1) else {
            crate::invariant_violated("value stack underflow");
        };
        let Some(&value) = self.slots.get(top) else {
            crate::invariant_violated("value stack height beyond initialized storage");
        };
        self.top = top;
        value
    }

    /// Returns the last live value without removing it.
    #[inline(always)]
    pub(crate) fn last(&self) -> &T {
        let Some(index) = self.top.checked_sub(1) else {
            crate::invariant_violated("value stack underflow");
        };
        self.get(index)
    }

    /// Returns a live value, rejecting stale slots above the height.
    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> &T {
        if index >= self.top {
            crate::invariant_violated("value stack index out of range");
        }
        match self.slots.get(index) {
            Some(value) => value,
            None => crate::invariant_violated("value stack index beyond initialized storage"),
        }
    }

    /// Updates a live value, rejecting stale slots above the height.
    #[inline(always)]
    pub(crate) fn set(&mut self, index: usize, value: T) {
        if index >= self.top {
            crate::invariant_violated("value stack index out of range");
        }
        match self.slots.get_mut(index) {
            Some(slot) => *slot = value,
            None => crate::invariant_violated("value stack index beyond initialized storage"),
        }
    }

    /// Copies a value between live slots.
    #[inline(always)]
    pub(crate) fn copy(&mut self, from: usize, to: usize) {
        let value = *self.get(from);
        self.set(to, value);
    }

    /// Truncates to `n`, preserving up to `end_keep` values from the old tail.
    #[inline(always)]
    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        if n > self.top {
            crate::invariant_violated("value stack truncate beyond live values");
        }
        let keep = (self.top - n).min(end_keep);
        self.slots.copy_within(self.top - keep..self.top, n);
        self.top = n + keep;
    }

    /// Removes live values above `n` without changing storage.
    #[inline(always)]
    pub(crate) fn truncate_to(&mut self, n: usize) {
        if n > self.top {
            crate::invariant_violated("value stack truncate beyond live values");
        }
        self.top = n;
    }

    /// Copies a single return value to `n` and publishes its final height once.
    #[inline(always)]
    pub(crate) fn truncate_to_one_tail(&mut self, n: usize) {
        let value = *self.last();
        self.set(n, value);
        self.top = n + 1;
    }

    /// Checks a frame's final layout and reserves storage without changing live values.
    #[inline]
    pub(crate) fn prepare_locals(&mut self, base: usize, local_count: usize, max_stack: usize) -> Result<usize, Trap> {
        let Some(end) = base.checked_add(local_count) else {
            return Err(Trap::ValueStackOverflow);
        };
        let Some(required) = end.checked_add(max_stack) else {
            return Err(Trap::ValueStackOverflow);
        };
        self.reserve(required)?;
        if end > self.slots.len() {
            self.slots.resize(end, T::default());
        }
        Ok(end)
    }

    /// Zeroes a prepared frame's non-parameter locals and publishes its live height.
    #[inline]
    pub(crate) fn initialize_locals(&mut self, end: usize) {
        if end == self.top {
            return;
        }
        let Some(locals) = self.slots.get_mut(self.top..end) else {
            crate::invariant_violated("value stack locals beyond prepared storage");
        };
        locals.fill(T::default());
        self.top = end;
    }

    /// Selects one of two adjacent tuples of `count` live values.
    #[inline(always)]
    pub(crate) fn select_many(&mut self, count: usize, condition: bool) {
        let Some(needed) = count.checked_mul(2) else {
            crate::invariant_violated("value stack select count overflow");
        };
        if self.top < needed {
            crate::invariant_violated("value stack underflow");
        }
        if !condition {
            self.slots.copy_within(self.top - count..self.top, self.top - needed);
        }
        self.top -= count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_is_not_the_live_height() {
        let mut stack = StackLane::<u32>::new(StackConfig::fixed(4));
        assert_eq!((stack.slots.len(), stack.slots.capacity()), (0, 4));
        stack.prepare_locals(0, 0, 4).unwrap();
        assert_eq!((stack.len(), stack.slots.len()), (0, 0));
        stack.push(11);
        stack.push(22);
        assert_eq!(stack.pop(), 22);
        assert_eq!(stack.as_slice(), [11]);
        stack.clear();
        assert_eq!((stack.len(), stack.slots.len()), (0, 2));
        assert!(stack.as_slice().is_empty());
    }

    #[test]
    fn small_dynamic_growth() {
        let mut stack = StackLane::<u32>::new(StackConfig::dynamic(0, 16));
        assert_eq!((stack.slots.len(), stack.slots.capacity()), (0, 0));
        stack.push_or_grow(11).unwrap();
        let allocation = stack.slots.as_ptr();
        assert_eq!(stack.slots.capacity(), MIN_DYNAMIC_CAPACITY);
        stack.push_or_grow(22).unwrap();
        assert_eq!(stack.slots.as_ptr(), allocation);
        assert_eq!(stack.as_slice(), [11, 22]);

        let mut limited = StackLane::<u32>::new(StackConfig::dynamic(0, 1));
        limited.push_or_grow(33).unwrap();
        assert_eq!(limited.slots.capacity(), 1);
        assert!(matches!(limited.push_or_grow(44), Err(Trap::ValueStackOverflow)));
    }

    #[test]
    fn only_locals_are_initialized() {
        let mut stack = StackLane::<u32>::new(StackConfig::fixed(8));
        let allocation = stack.slots.as_ptr();
        let end = stack.prepare_locals(0, 1, 2).unwrap();
        assert_eq!(stack.slots.as_ptr(), allocation);
        assert_eq!((stack.len(), stack.slots.len(), stack.slots.capacity()), (0, 1, 8));
        stack.initialize_locals(end);
        stack.push(11);
        assert_eq!(stack.as_slice(), [0, 11]);
        stack.clear();
        stack.prepare_locals(0, 2, 3).unwrap();
        assert_eq!(stack.slots.as_ptr(), allocation);
        assert_eq!((stack.len(), stack.slots.len(), stack.slots.capacity()), (0, 2, 8));
        assert!(matches!(stack.prepare_locals(0, 9, 0), Err(Trap::ValueStackOverflow)));
    }

    #[test]
    fn spare_capacity_does_not_raise_the_limit() {
        let mut stack = StackLane::<u32>::new(StackConfig::dynamic(0, 2));
        stack.slots.reserve(32);
        let allocation = stack.slots.as_ptr();
        stack.push_or_grow(11).unwrap();
        assert_eq!(stack.slots.as_ptr(), allocation);
        stack.push_or_grow(22).unwrap();
        assert!(matches!(stack.push_or_grow(33), Err(Trap::ValueStackOverflow)));
        assert_eq!(stack.as_slice(), [11, 22]);
        assert_eq!(stack.slots.len(), 2);
    }

    #[test]
    fn frame_layout_checks_arithmetic() {
        let mut stack = StackLane::<u32>::new(StackConfig::dynamic(0, usize::MAX));
        assert!(matches!(stack.prepare_locals(usize::MAX, 1, 0), Err(Trap::ValueStackOverflow)));
        assert!(matches!(stack.prepare_locals(1, 0, usize::MAX), Err(Trap::ValueStackOverflow)));
        #[cfg(target_pointer_width = "64")]
        assert!(matches!(stack.prepare_locals(u32::MAX as usize + 1, 0, 0), Err(Trap::ValueStackOverflow)));
        assert_eq!((stack.len(), stack.slots.capacity()), (0, 0));
    }

    #[test]
    fn stack_operations_match_vec() {
        let mut stack = StackLane::<u32>::new(StackConfig::fixed(128));
        stack.prepare_locals(0, 0, 128).unwrap();
        let mut values = Vec::new();
        let mut random = 1u32;
        for _ in 0..10_000 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let len = values.len();
            match random % 7 {
                0 if len < 128 => {
                    stack.push(random);
                    values.push(random);
                }
                1 if len > 0 => assert_eq!(stack.pop(), values.pop().unwrap()),
                2 if len > 0 => {
                    let index = random as usize % len;
                    stack.set(index, random);
                    values[index] = random;
                }
                3 if len > 0 => {
                    let destination = random as usize % len;
                    stack.truncate_to_one_tail(destination);
                    values[destination] = *values.last().unwrap();
                    values.truncate(destination + 1);
                }
                4 if len > 0 => {
                    let destination = random as usize % len;
                    let count = (len - destination).min(3);
                    stack.truncate_keep(destination, count);
                    values.copy_within(len - count.., destination);
                    values.truncate(destination + count);
                }
                5 if len >= 2 => {
                    let count = (random as usize % (len / 2)) + 1;
                    let condition = random & 8 != 0;
                    stack.select_many(count, condition);
                    if !condition {
                        values.copy_within(len - count.., len - 2 * count);
                    }
                    values.truncate(len - count);
                }
                _ => {}
            }
            assert_eq!(stack.as_slice(), values);
            assert!(stack.slots.len() <= 128);
        }
    }

    #[cfg(any(debug_assertions, not(tw_tail_calls)))]
    #[test]
    #[should_panic(expected = "value stack index out of range")]
    fn get_rejects_popped_slots() {
        let mut stack = StackLane::<u32>::new(StackConfig::fixed(4));
        stack.push_or_grow(11).unwrap();
        stack.pop();
        stack.get(0);
    }

    #[cfg(any(debug_assertions, not(tw_tail_calls)))]
    #[test]
    #[should_panic(expected = "value stack index out of range")]
    fn set_rejects_popped_slots() {
        let mut stack = StackLane::<u32>::new(StackConfig::fixed(4));
        stack.push_or_grow(11).unwrap();
        stack.pop();
        stack.set(0, 22);
    }
}
