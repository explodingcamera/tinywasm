use crate::{runtime::RawWasmValue, Error, Result};
use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::{ValType, WasmValue};

// minimum call stack size
pub const CALL_STACK_SIZE: usize = 1024;

#[derive(Debug)]
pub struct CallStack {
    stack: Vec<CallFrame<true>>,
    top: usize,
}

impl Default for CallStack {
    fn default() -> Self {
        Self {
            stack: Vec::with_capacity(CALL_STACK_SIZE),
            top: 0,
        }
    }
}

impl CallStack {
    #[inline]
    pub(crate) fn _top(&self) -> Result<&CallFrame<true>> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn top_mut(&mut self) -> Result<&mut CallFrame<true>> {
        assert!(self.top <= self.stack.len());
        if self.top == 0 {
            return Err(Error::CallStackEmpty);
        }
        Ok(&mut self.stack[self.top - 1])
    }

    #[inline]
    pub(crate) fn push(&mut self, call_frame: CallFrame<true>) {
        self.top += 1;
        self.stack.push(call_frame);
    }
}

#[derive(Debug)]
pub struct CallFrame<const CHECK: bool> {
    pub instr_ptr: usize,
    pub func_ptr: usize,

    pub locals: Box<[RawWasmValue]>,
    pub local_count: usize,
}

impl<const CHECK: bool> CallFrame<CHECK> {
    pub fn new(func_ptr: usize, params: &[WasmValue], local_types: Vec<ValType>) -> Self {
        let mut locals = Vec::with_capacity(local_types.len() + params.len());
        locals.extend(params.iter().map(|v| RawWasmValue::from(*v)));
        locals.extend(local_types.iter().map(|_| RawWasmValue::default()));

        Self {
            instr_ptr: 0,
            func_ptr,
            local_count: locals.len(),
            locals: locals.into_boxed_slice(),
        }
    }

    #[inline]
    pub(crate) fn set_local(&mut self, local_index: usize, value: RawWasmValue) {
        if local_index >= self.local_count {
            panic!("Invalid local index");
        }

        self.locals[local_index] = value;
    }

    #[inline]
    pub(crate) fn get_local(&self, local_index: usize) -> RawWasmValue {
        if local_index >= self.local_count {
            panic!("Invalid local index");
        }

        self.locals[local_index]
    }
}
