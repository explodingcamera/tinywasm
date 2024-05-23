use crate::{cold, runtime::WasmValueRepr, unlikely, Error, Result};
use alloc::vec::Vec;
use tinywasm_types::{ValType, WasmValue};

pub(crate) const MIN_VALUE_STACK_SIZE: usize = 1024 * 128;

#[derive(Debug)]
pub(crate) struct ValueStack<T>(Vec<T>);

impl<T> Default for ValueStack<T> {
    fn default() -> Self {
        Self(Vec::with_capacity(MIN_VALUE_STACK_SIZE))
    }
}

impl<T: From<WasmValue> + Copy + WasmValueRepr> ValueStack<T> {
    #[inline]
    pub(crate) fn extend_from_typed(&mut self, values: &[WasmValue]) {
        self.0.extend(values.iter().map(|v| T::from(*v)));
    }

    #[inline(always)]
    pub(crate) fn replace_top(&mut self, func: fn(T) -> T) -> Result<()> {
        let v = self.last_mut()?;
        *v = func(*v);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn calculate(&mut self, func: fn(T, T) -> T) -> Result<()> {
        let v2 = self.pop()?;
        let v1 = self.last_mut()?;
        *v1 = func(*v1, v2);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn calculate_trap(&mut self, func: fn(T, T) -> Result<T>) -> Result<()> {
        let v2 = self.pop()?;
        let v1 = self.last_mut()?;
        *v1 = func(*v1, v2)?;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub(crate) fn truncate_keep(&mut self, n: u32, end_keep: u32) {
        let total_to_keep = n + end_keep;
        let len = self.0.len() as u32;
        assert!(len >= total_to_keep, "Total to keep should be less than or equal to self.top");

        if len <= total_to_keep {
            return; // No need to truncate if the current size is already less than or equal to total_to_keep
        }

        let items_to_remove = len - total_to_keep;
        let remove_start_index = (len - items_to_remove - end_keep) as usize;
        let remove_end_index = (len - end_keep) as usize;
        self.0.drain(remove_start_index..remove_end_index);
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) {
        self.0.push(value);
    }

    #[inline(always)]
    pub(crate) fn extend_from_slice(&mut self, values: &[T]) {
        self.0.extend_from_slice(values);
    }

    #[inline]
    pub(crate) fn last_mut(&mut self) -> Result<&mut T> {
        match self.0.last_mut() {
            Some(v) => Ok(v),
            None => {
                cold();
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline]
    pub(crate) fn last(&self) -> Result<&T> {
        match self.0.last() {
            Some(v) => Ok(v),
            None => {
                cold();
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> Result<T> {
        match self.0.pop() {
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
        let end = self.0.len() - result_count as usize;
        self.0.drain(start..end);
    }

    #[inline]
    pub(crate) fn last_n(&self, n: usize) -> Result<&[T]> {
        let len = self.0.len();
        if unlikely(len < n) {
            return Err(Error::ValueStackUnderflow);
        }
        Ok(&self.0[len - n..len])
    }

    #[inline]
    pub(crate) fn pop_n_rev(&mut self, n: usize) -> Result<alloc::vec::Drain<'_, T>> {
        if unlikely(self.0.len() < n) {
            return Err(Error::ValueStackUnderflow);
        }
        Ok(self.0.drain((self.0.len() - n)..))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RawWasmValue;

    #[test]
    fn test_value_stack() {
        let mut stack: ValueStack<RawWasmValue> = ValueStack::default();
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
                let mut stack: ValueStack<RawWasmValue> = ValueStack::default();
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
