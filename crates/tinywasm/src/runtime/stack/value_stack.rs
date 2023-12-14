use crate::{runtime::RawWasmValue, Error, Result};
use alloc::vec::Vec;
use tinywasm_types::BlockArgs;

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
    pub(crate) fn last_mut(&mut self) -> Option<&mut RawWasmValue> {
        self.stack.last_mut()
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        assert!(self.top <= self.stack.len());
        self.top
    }

    #[inline]
    pub(crate) fn trim(&mut self, n: usize) {
        assert!(self.top <= self.stack.len());
        self.top -= n;
        self.stack.truncate(self.top);
    }

    #[inline]
    pub(crate) fn push_block_args(&self, args: BlockArgs) -> Result<()> {
        match args {
            BlockArgs::Empty => Ok(()),
            BlockArgs::Type(_t) => todo!(),
            BlockArgs::FuncType(_t) => todo!(),
        }
    }

    #[inline]
    pub(crate) fn extend(&mut self, values: impl IntoIterator<Item = RawWasmValue> + ExactSizeIterator) {
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
        Ok(self.pop()?.into())
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
