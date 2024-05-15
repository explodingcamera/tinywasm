use crate::{cold, runtime::RawWasmValue, unlikely, Error, Result};
use alloc::vec::Vec;
use tinywasm_types::{ValType, WasmValue};

pub(crate) const MIN_VALUE_STACK_SIZE: usize = 1024 * 128;

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
    pub(crate) fn extend_from_typed(&mut self, values: &[WasmValue]) {
        self.stack.extend(values.iter().map(|v| RawWasmValue::from(*v)));
    }

    #[inline(always)]
    pub(crate) fn replace_top(&mut self, func: fn(RawWasmValue) -> RawWasmValue) -> Result<()> {
        let v = self.last_mut()?;
        *v = func(*v);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn calculate(&mut self, func: fn(RawWasmValue, RawWasmValue) -> RawWasmValue) -> Result<()> {
        let v2 = self.pop()?;
        let v1 = self.last_mut()?;
        *v1 = func(*v1, v2);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn calculate_trap(
        &mut self,
        func: fn(RawWasmValue, RawWasmValue) -> Result<RawWasmValue>,
    ) -> Result<()> {
        let v2 = self.pop()?;
        let v1 = self.last_mut()?;
        *v1 = func(*v1, v2)?;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.stack.len()
    }

    #[inline]
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
    pub(crate) fn last_mut(&mut self) -> Result<&mut RawWasmValue> {
        match self.stack.last_mut() {
            Some(v) => Ok(v),
            None => {
                cold();
                Err(Error::ValueStackUnderflow)
            }
        }
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

    #[inline(always)]
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
        Ok(self.pop_n_rev(types.len())?.zip(types.iter()).map(|(v, ty)| v.attach_type(*ty)).collect())
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
        if unlikely(self.stack.len() < n) {
            return Err(Error::ValueStackUnderflow);
        }
        Ok(self.stack.drain((self.stack.len() - n)..))
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
        assert_eq!(i32::from(stack.pop().unwrap()), 3);
        assert_eq!(stack.len(), 2);
        assert_eq!(i32::from(stack.pop().unwrap()), 2);
        assert_eq!(stack.len(), 1);
        assert_eq!(i32::from(stack.pop().unwrap()), 1);
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn test_truncate_keep() {
        macro_rules! test_macro {
            ($( $n:expr, $end_keep:expr, $expected:expr ),*) => {
            $(
                let mut stack = ValueStack::default();
                stack.push(1.into());
                stack.push(2.into());
                stack.push(3.into());
                stack.push(4.into());
                stack.push(5.into());
                stack.truncate_keep($n, $end_keep);
                assert_eq!(stack.len(), $expected);
            )*
            };
        }

        test_macro! {
            0, 0, 0,
            1, 0, 1,
            0, 1, 1,
            1, 1, 2,
            2, 1, 3,
            2, 2, 4
        }
    }
}
