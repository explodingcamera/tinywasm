use crate::{boxvec::BoxVec, cold, runtime::RawWasmValue, unlikely, Error, Result};
use alloc::{borrow::Cow, vec::Vec};
use tinywasm_types::{ValType, WasmValue};

use super::BlockFrame;

pub(crate) const MIN_VALUE_STACK_SIZE: usize = 1024 * 128;
// pub(crate) const MAX_VALUE_STACK_SIZE: usize = u32::MAX / 32 as usize;

#[cfg(feature = "simd")]
pub(crate) const MIN_SIMD_VALUE_STACK_SIZE: usize = 1024 * 32;

// #[cfg(feature = "simd")]
// pub(crate) const MAX_SIMD_VALUE_STACK_SIZE: usize = u16::MAX as usize;

#[cfg(feature = "simd")]
use crate::runtime::raw_simd::RawSimdWasmValue;

#[derive(Debug)]
pub(crate) struct ValueStack {
    pub(crate) stack: BoxVec<RawWasmValue>,

    #[cfg(feature = "simd")]
    simd_stack: BoxVec<RawSimdWasmValue>,
}

impl Default for ValueStack {
    fn default() -> Self {
        Self {
            stack: BoxVec::with_capacity(MIN_VALUE_STACK_SIZE),

            #[cfg(feature = "simd")]
            simd_stack: BoxVec::with_capacity(MIN_SIMD_VALUE_STACK_SIZE),
        }
    }
}

impl ValueStack {
    #[inline]
    pub(crate) fn extend_from_typed(&mut self, values: &[WasmValue]) {
        #[cfg(not(feature = "simd"))]
        self.stack.extend(values.iter().map(|v| RawWasmValue::from(*v)));

        #[cfg(feature = "simd")]
        {
            values.iter().for_each(|v| match v {
                WasmValue::V128(v) => self.simd_stack.push(RawSimdWasmValue::from(*v)),
                v => self.stack.push(RawWasmValue::from(*v)),
            });
        }
    }

    #[inline(always)]
    pub(crate) fn replace_top(&mut self, func: fn(RawWasmValue) -> RawWasmValue) -> Result<()> {
        let v = self.last_mut()?;
        *v = func(*v);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn calculate(&mut self, func: fn(RawWasmValue, RawWasmValue) -> RawWasmValue) -> Result<()> {
        if self.stack.end < 2 {
            cold(); // cold in here instead of the stack makes a huge performance difference
            return Err(Error::ValueStackUnderflow);
        }

        assert!(
            self.stack.end >= 2 && self.stack.end <= self.stack.data.len(),
            "invalid stack state (should be impossible)"
        );

        self.stack.data[self.stack.end - 2] =
            func(self.stack.data[self.stack.end - 2], self.stack.data[self.stack.end - 1]);

        self.stack.end -= 1;
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

    #[cfg(feature = "simd")]
    #[inline(always)]
    pub(crate) fn simd_len(&self) -> usize {
        self.simd_stack.len()
    }

    #[inline]
    pub(crate) fn truncate_keep(&mut self, n: u32, end_keep: u32) {
        truncate_keep(&mut self.stack, n, end_keep);
    }

    #[cfg(feature = "simd")]
    #[inline]
    pub(crate) fn truncate_keep_simd(&mut self, n: u16, end_keep: u32) {
        truncate_keep(&mut self.simd_stack, n as u32, end_keep);
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: RawWasmValue) {
        self.stack.push(value);
    }

    #[inline(always)]
    pub(crate) fn extend_from_slice(&mut self, values: &[RawWasmValue]) {
        self.stack.extend_from_slice(values);
    }

    #[inline]
    pub(crate) fn last_mut(&mut self) -> Result<&mut RawWasmValue> {
        match self.stack.last_mut() {
            Some(v) => Ok(v),
            None => {
                cold(); // cold in here instead of the stack makes a huge performance difference
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline]
    pub(crate) fn last(&self) -> Result<&RawWasmValue> {
        match self.stack.last() {
            Some(v) => Ok(v),
            None => {
                cold(); // cold in here instead of the stack makes a huge performance difference
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> Result<RawWasmValue> {
        match self.stack.pop() {
            Some(v) => Ok(v),
            None => {
                cold(); // cold in here instead of the stack makes a huge performance difference
                Err(Error::ValueStackUnderflow)
            }
        }
    }

    #[inline]
    pub(crate) fn pop_params(&mut self, types: &[ValType]) -> Result<Vec<WasmValue>> {
        #[cfg(not(feature = "simd"))]
        return Ok(self.pop_n_rev(types.len())?.zip(types.iter()).map(|(v, ty)| v.attach_type(*ty)).collect());

        #[cfg(feature = "simd")]
        {
            let mut values = Vec::with_capacity(types.len());
            for ty in types {
                match ty {
                    ValType::V128 => values.push(WasmValue::V128(self.simd_stack.pop().unwrap().into())),
                    ty => values.push(self.pop()?.attach_type(*ty)),
                }
            }
            Ok(values)
        }
    }

    #[inline]
    pub(crate) fn break_to_results(&mut self, bf: &BlockFrame) {
        let end = self.stack.len() - bf.results as usize;
        self.stack.drain(bf.stack_ptr as usize..end);

        #[cfg(feature = "simd")]
        let end = self.simd_stack.len() - bf.simd_results as usize;
        #[cfg(feature = "simd")]
        self.simd_stack.drain(bf.simd_stack_ptr as usize..end);
    }

    #[inline]
    pub(crate) fn break_to_params(&mut self, bf: &BlockFrame) {
        let end = self.stack.len() - bf.params as usize;
        self.stack.drain(bf.stack_ptr as usize..end);

        #[cfg(feature = "simd")]
        let end = self.simd_stack.len() - bf.simd_params as usize;
        #[cfg(feature = "simd")]
        self.simd_stack.drain(bf.simd_stack_ptr as usize..end);
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
    pub(crate) fn pop_n_rev(&mut self, n: usize) -> Result<Cow<'_, [RawWasmValue]>> {
        if unlikely(self.stack.len() < n) {
            return Err(Error::ValueStackUnderflow);
        }
        Ok(self.stack.drain((self.stack.len() - n)..))
    }
}

#[inline(always)]
fn truncate_keep<T: Copy + Default>(data: &mut BoxVec<T>, n: u32, end_keep: u32) {
    let total_to_keep = n + end_keep;
    let len = data.len() as u32;
    assert!(len >= total_to_keep, "RawWasmValueotal to keep should be less than or equal to self.top");

    if len <= total_to_keep {
        return; // No need to truncate if the current size is already less than or equal to total_to_keep
    }

    let items_to_remove = len - total_to_keep;
    let remove_start_index = (len - items_to_remove - end_keep) as usize;
    let remove_end_index = (len - end_keep) as usize;
    data.drain(remove_start_index..remove_end_index);
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
