use core::ops::Range;

use crate::{runtime::RawWasmValue, Error, Result};
use alloc::vec::Vec;
use log::info;

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
    #[cfg(test)]
    pub(crate) fn data(&self) -> &[RawWasmValue] {
        &self.stack
    }

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

    #[inline]
    pub(crate) fn _truncate(&mut self, n: usize) {
        assert!(self.top <= self.stack.len());
        self.top -= n;
        self.stack.truncate(self.top);
    }

    #[inline]
    // example: [1, 2, 3] n=1, end_keep=1 => [1, 3]
    // example: [1] n=1, end_keep=1 => [1]
    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        if n == end_keep || n == 0 {
            return;
        }

        assert!(self.top <= self.stack.len());
        info!("removing from {} to {}", self.top - n, self.top - end_keep);
        self.stack.drain(self.top - n..self.top - end_keep);
        self.top -= n - end_keep;
    }

    #[inline]
    pub(crate) fn _extend(&mut self, values: impl IntoIterator<Item = RawWasmValue> + ExactSizeIterator) {
        self.top += values.len();
        self.stack.extend(values);
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
    use crate::std::panic;

    fn crate_stack<T: Into<RawWasmValue> + Copy>(data: &[T]) -> ValueStack {
        let mut stack = ValueStack::default();
        stack._extend(data.iter().map(|v| (*v).into()));
        stack
    }

    fn assert_truncate_keep<T: Into<RawWasmValue> + Copy>(data: &[T], n: usize, end_keep: usize, expected: &[T]) {
        let mut stack = crate_stack(data);
        stack.truncate_keep(n, end_keep);
        assert_eq!(
            stack.data(),
            expected.iter().map(|v| (*v).into()).collect::<Vec<_>>().as_slice()
        );
    }

    fn catch_unwind_silent<F: FnOnce() -> R + panic::UnwindSafe, R>(f: F) -> crate::std::thread::Result<R> {
        let prev_hook = panic::take_hook();
        panic::set_hook(alloc::boxed::Box::new(|_| {}));
        let result = panic::catch_unwind(f);
        panic::set_hook(prev_hook);
        result
    }

    #[test]
    fn test_truncate_keep() {
        assert_truncate_keep(&[1, 2, 3], 1, 1, &[1, 2, 3]);
        assert_truncate_keep(&[1], 1, 1, &[1]);
        assert_truncate_keep(&[1, 2, 3], 2, 1, &[1, 3]);
        assert_truncate_keep::<i32>(&[], 0, 0, &[]);
        catch_unwind_silent(|| assert_truncate_keep(&[1, 2, 3], 4, 1, &[1, 3])).expect_err("should panic");
    }

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
