use core::ops::Range;

use crate::{runtime::RawWasmValue, Error, Result};
use alloc::vec::Vec;

// minimum stack size
pub(crate) const STACK_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct ValueStack {
    stack: Vec<RawWasmValue>,

    // TODO: don't pop the stack, just keep track of the top for better performance
    top: usize,
}

impl Default for ValueStack {
    fn default() -> Self {
        Self {
            stack: Vec::with_capacity(STACK_SIZE),
            top: 0,
        }
    }
}

impl ValueStack {
    #[inline]
    pub(crate) fn extend_from_within(&mut self, range: Range<usize>) {
        self.top += range.len();
        self.stack.extend_from_within(range);
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        assert!(self.top <= self.stack.len());
        self.top
    }

    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        let total_to_keep = n + end_keep;
        assert!(
            self.top >= total_to_keep,
            "Total to keep should be less than or equal to self.top"
        );

        let current_size = self.stack.len();
        if current_size <= total_to_keep {
            return; // No need to truncate if the current size is already less than or equal to total_to_keep
        }

        let items_to_remove = current_size - total_to_keep;
        let remove_start_index = self.top - items_to_remove - end_keep;
        let remove_end_index = self.top - end_keep;

        self.stack.drain(remove_start_index..remove_end_index);
        self.top = total_to_keep; // Update top to reflect the new size
    }

    #[inline]
    pub(crate) fn push(&mut self, value: RawWasmValue) {
        self.top += 1;
        self.stack.push(value);
    }

    #[inline]
    pub(crate) fn last(&self) -> Result<&RawWasmValue> {
        self.stack.last().ok_or(Error::StackUnderflow)
    }

    #[inline]
    pub(crate) fn pop_t<T: From<RawWasmValue>>(&mut self) -> Result<T> {
        self.top -= 1;
        Ok(self.stack.pop().ok_or(Error::StackUnderflow)?.into())
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Result<RawWasmValue> {
        self.top -= 1;
        self.stack.pop().ok_or(Error::StackUnderflow)
    }

    pub(crate) fn break_to(&mut self, new_stack_size: usize, result_count: usize) {
        self.stack
            .copy_within((self.top - result_count)..self.top, new_stack_size);
        self.top = new_stack_size + result_count;
        self.stack.truncate(self.top);
    }

    #[inline]
    pub(crate) fn last_n(&self, n: usize) -> Result<&[RawWasmValue]> {
        if self.top < n {
            return Err(Error::StackUnderflow);
        }
        Ok(&self.stack[self.top - n..self.top])
    }

    #[inline]
    pub(crate) fn pop_n(&mut self, n: usize) -> Result<Vec<RawWasmValue>> {
        if self.top < n {
            return Err(Error::StackUnderflow);
        }
        self.top -= n;
        let res = self.stack.drain(self.top..).rev().collect::<Vec<_>>();
        Ok(res)
    }

    #[inline]
    pub(crate) fn pop_n_const<const N: usize>(&mut self) -> Result<[RawWasmValue; N]> {
        if self.top < N {
            return Err(Error::StackUnderflow);
        }
        self.top -= N;
        let mut res = [RawWasmValue::default(); N];
        for i in res.iter_mut().rev() {
            *i = self.stack.pop().ok_or(Error::InvalidStore)?;
        }

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_stack() {
        let mut stack = ValueStack::default();
        stack.push(1.into());
        stack.push(2.into());
        stack.push(3.into());
        assert_eq!(stack.len(), 3);
        assert_eq!(stack.pop_t::<i32>().unwrap(), 3);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.pop_t::<i32>().unwrap(), 2);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.pop_t::<i32>().unwrap(), 1);
        assert_eq!(stack.len(), 0);
    }
}
