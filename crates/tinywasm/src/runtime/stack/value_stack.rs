use crate::{runtime::RawWasmValue, Error, Result};
use alloc::vec::Vec;

// minimum stack size
pub const STACK_SIZE: usize = 1024;

#[derive(Debug)]
pub struct ValueStack {
    stack: Vec<RawWasmValue>,
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
    pub(crate) fn pop(&mut self) -> Option<RawWasmValue> {
        self.top -= 1;
        self.stack.pop()
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
