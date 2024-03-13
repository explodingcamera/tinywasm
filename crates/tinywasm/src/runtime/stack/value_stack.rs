use core::ops::Range;

use crate::{cold, runtime::RawWasmValue, unlikely, Error, Result};
use alloc::vec::Vec;
use tinywasm_types::{ValType, WasmValue};

pub(crate) const MIN_VALUE_STACK_SIZE: usize = 1024;
// pub(crate) const MAX_VALUE_STACK_SIZE: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct ValueStack {
    stack: Vec<RawWasmValue>,
}

impl Default for ValueStack {
    fn default() -> Self {
        Self { stack: Vec::with_capacity(MIN_VALUE_STACK_SIZE) }
    }
}

impl ValueStack {
    #[inline]
    pub(crate) fn extend_from_within(&mut self, range: Range<usize>) {
        self.stack.extend_from_within(range);
    }

    #[inline]
    pub(crate) fn extend_from_typed(&mut self, values: &[WasmValue]) {
        self.stack.extend(values.iter().map(|v| RawWasmValue::from(*v)));
    }

    #[inline]
    pub(crate) fn extend_from_slice(&mut self, values: &[RawWasmValue]) {
        self.stack.extend_from_slice(values);
    }

    #[inline]
    pub(crate) fn replace_top(&mut self, func: impl FnOnce(RawWasmValue) -> RawWasmValue) {
        let len = self.stack.len();
        if unlikely(len == 0) {
            return;
        }
        let top = self.stack[len - 1];
        self.stack[len - 1] = func(top);
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.stack.len()
    }

    pub(crate) fn truncate_keep(&mut self, n: u32, end_keep: u32) {
        let total_to_keep = n + end_keep;
        let len = self.stack.len() as u32;
        assert!(len >= total_to_keep, "Total to keep should be less than or equal to self.top");

        if len <= total_to_keep {
            return; // No need to truncate if the current size is already less than or equal to total_to_keep
        }

        let items_to_remove = len - total_to_keep;
        let remove_start_index = (len - items_to_remove - end_keep) as usize;
        let remove_end_index = (len - end_keep) as usize;
        self.stack.drain(remove_start_index..remove_end_index);
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: RawWasmValue) {
        self.stack.push(value);
    }

    #[inline]
    pub(crate) fn last(&self) -> Result<&RawWasmValue> {
        match self.stack.last() {
            Some(v) => Ok(v),
            None => {
                cold();
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline]
    pub(crate) fn pop_t<T: From<RawWasmValue>>(&mut self) -> Result<T> {
        match self.stack.pop() {
            Some(v) => Ok(v.into()),
            None => {
                cold();
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline]
    pub(crate) fn pop(&mut self) -> Result<RawWasmValue> {
        match self.stack.pop() {
            Some(v) => Ok(v),
            None => {
                cold();
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline]
    pub(crate) fn pop_params(&mut self, types: &[ValType]) -> Result<Vec<WasmValue>> {
        let res = self.pop_n_rev(types.len())?.zip(types.iter()).map(|(v, ty)| v.attach_type(*ty)).collect();
        Ok(res)
    }

    #[inline]
    pub(crate) fn break_to(&mut self, new_stack_size: u32, result_count: u8) {
        let start = new_stack_size as usize;
        let end = self.stack.len() - result_count as usize;
        self.stack.drain(start..end);
    }

    #[inline]
    pub(crate) fn last_n(&self, n: usize) -> Result<&[RawWasmValue]> {
        let len = self.stack.len();
        if unlikely(len < n) {
            return Err(Error::ValueStackUnderflow);
        }
        Ok(&self.stack[len - n..len])
    }

    #[inline]
    pub(crate) fn pop_n_rev(&mut self, n: usize) -> Result<alloc::vec::Drain<'_, RawWasmValue>> {
        let len = self.stack.len();
        if unlikely(len < n) {
            return Err(Error::ValueStackUnderflow);
        }
        let res = self.stack.drain((len - n)..);
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
