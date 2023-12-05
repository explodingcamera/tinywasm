use alloc::vec::Vec;

mod call;
pub use call::CallFrame;
use tinywasm_types::WasmValue;

use crate::{Error, Result};

// minimum stack size
pub const STACK_SIZE: usize = 1024;
// minimum call stack size
pub const CALL_STACK_SIZE: usize = 1024;

/// A WebAssembly Stack
#[derive(Debug)]
pub struct Stack {
    // keeping this typed for now to make it easier to debug
    // TODO: Maybe split into Vec<u8> and Vec<ValType> for better memory usage?
    pub(crate) values: ValueStack,

    /// The call stack
    pub(crate) call_stack: CallStack,
}

#[derive(Debug)]
pub struct CallStack {
    stack: Vec<CallFrame>,
    top: usize,
}

#[derive(Debug)]
pub struct ValueStack {
    stack: Vec<WasmValue>,
    top: usize,
}

impl ValueStack {
    #[inline]
    pub(crate) fn _extend(&mut self, values: &[WasmValue]) {
        self.top += values.len();
        self.stack.extend(values.iter().cloned());
    }

    #[inline]
    pub(crate) fn push(&mut self, value: WasmValue) {
        self.top += 1;
        self.stack.push(value);
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Option<WasmValue> {
        self.top -= 1;
        self.stack.pop()
    }

    #[inline]
    pub(crate) fn pop_n(&mut self, n: usize) -> Result<Vec<WasmValue>> {
        if self.top < n {
            return Err(Error::StackUnderflow);
        }
        self.top -= n;
        let res = self.stack.drain(self.top..).rev().collect::<Vec<_>>();
        Ok(res)
    }

    #[inline]
    pub(crate) fn pop_n_const<const N: usize>(&mut self) -> Result<[WasmValue; N]> {
        if self.top < N {
            return Err(Error::StackUnderflow);
        }
        self.top -= N;
        let mut res = [WasmValue::I32(0); N];
        for i in res.iter_mut().rev() {
            *i = self.stack.pop().ok_or(Error::InvalidStore)?;
        }

        Ok(res)
    }
}

impl CallStack {
    #[inline]
    pub(crate) fn _top(&self) -> Result<&CallFrame> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn top_mut(&mut self) -> Result<&mut CallFrame> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&mut self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame) {
        self.top += 1;
        self.stack.push(call_frame);
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            values: ValueStack {
                stack: Vec::with_capacity(STACK_SIZE),
                top: 0,
            },
            call_stack: CallStack {
                stack: Vec::with_capacity(CALL_STACK_SIZE),
                top: 0,
            },
        }
    }
}
