use alloc::vec::Vec;
use tinywasm_types::{ValType, WasmValue};

use super::values::*;
use crate::Result;
pub(crate) const STACK_32_SIZE: usize = 1024 * 128;
pub(crate) const STACK_64_SIZE: usize = 1024 * 128;
pub(crate) const STACK_128_SIZE: usize = 1024 * 128;
pub(crate) const STACK_REF_SIZE: usize = 1024;

#[derive(Debug)]
pub(crate) struct ValueStack {
    pub(crate) stack_32: Vec<Value32>,
    pub(crate) stack_64: Vec<Value64>,
    pub(crate) stack_128: Vec<Value128>,
    pub(crate) stack_ref: Vec<ValueRef>,
}

impl ValueStack {
    pub(crate) fn new() -> Self {
        Self {
            stack_32: Vec::with_capacity(STACK_32_SIZE),
            stack_64: Vec::with_capacity(STACK_64_SIZE),
            stack_128: Vec::with_capacity(STACK_128_SIZE),
            stack_ref: Vec::with_capacity(STACK_REF_SIZE),
        }
    }

    pub(crate) fn height(&self) -> StackLocation {
        StackLocation {
            s32: self.stack_32.len() as u32,
            s64: self.stack_64.len() as u32,
            s128: self.stack_128.len() as u32,
            sref: self.stack_ref.len() as u32,
        }
    }

    pub(crate) fn peek<T: InternalValue>(&self) -> Result<T> {
        T::stack_peek(self)
    }

    pub(crate) fn pop<T: InternalValue>(&mut self) -> Result<T> {
        T::stack_pop(self)
    }

    pub(crate) fn push<T: InternalValue>(&mut self, value: T) {
        T::stack_push(self, value)
    }

    pub(crate) fn drop<T: InternalValue>(&mut self) -> Result<()> {
        T::stack_pop(self).map(|_| ())
    }

    pub(crate) fn select<T: InternalValue>(&mut self) -> Result<()> {
        let cond: i32 = self.pop()?;
        let val2: T = self.pop()?;
        if cond == 0 {
            self.drop::<T>()?;
            self.push(val2);
        }
        Ok(())
    }

    pub(crate) fn calculate<T: InternalValue, U: InternalValue>(&mut self, func: fn(T, T) -> Result<U>) -> Result<()> {
        let v2 = T::stack_pop(self)?;
        let v1 = T::stack_pop(self)?;
        U::stack_push(self, func(v1, v2)?);
        Ok(())
    }

    pub(crate) fn replace_top<T: InternalValue, U: InternalValue>(&mut self, func: fn(T) -> Result<U>) -> Result<()> {
        let v1 = T::stack_pop(self)?;
        U::stack_push(self, func(v1)?);
        Ok(())
    }

    pub(crate) fn pop_dyn(&mut self, val_type: ValType) -> Result<TinyWasmValue> {
        match val_type {
            ValType::I32 => self.pop().map(TinyWasmValue::Value32),
            ValType::I64 => self.pop().map(TinyWasmValue::Value64),
            ValType::V128 => self.pop().map(TinyWasmValue::Value128),
            ValType::RefExtern => self.pop().map(TinyWasmValue::ValueRef),
            ValType::RefFunc => self.pop().map(TinyWasmValue::ValueRef),
            ValType::F32 => self.pop().map(TinyWasmValue::Value32),
            ValType::F64 => self.pop().map(TinyWasmValue::Value64),
        }
    }

    pub(crate) fn pop_many(&mut self, val_types: &[ValType]) -> Result<Vec<WasmValue>> {
        let mut values = Vec::with_capacity(val_types.len());
        for val_type in val_types.iter().rev() {
            values.push(self.pop_wasmvalue(*val_type)?);
        }
        Ok(values)
    }

    pub(crate) fn pop_many_raw(&mut self, val_types: &[ValType]) -> Result<Vec<TinyWasmValue>> {
        let mut values = Vec::with_capacity(val_types.len());
        for val_type in val_types.iter().rev() {
            values.push(self.pop_dyn(*val_type)?);
        }
        Ok(values)
    }

    pub(crate) fn truncate_keep(&mut self, height: &StackLocation, keep: &StackHeight) {
        truncate_keep(&mut self.stack_32, height.s32, keep.s32);
        truncate_keep(&mut self.stack_64, height.s64, keep.s64);
        truncate_keep(&mut self.stack_128, height.s128, keep.s128);
        truncate_keep(&mut self.stack_ref, height.sref, keep.sref);
    }

    pub(crate) fn push_dyn(&mut self, value: TinyWasmValue) {
        match value {
            TinyWasmValue::Value32(v) => self.stack_32.push(v),
            TinyWasmValue::Value64(v) => self.stack_64.push(v),
            TinyWasmValue::Value128(v) => self.stack_128.push(v),
            TinyWasmValue::ValueRef(v) => self.stack_ref.push(v),
        }
    }

    pub(crate) fn pop_wasmvalue(&mut self, val_type: ValType) -> Result<WasmValue> {
        match val_type {
            ValType::I32 => self.pop().map(WasmValue::I32),
            ValType::I64 => self.pop().map(WasmValue::I64),
            ValType::V128 => self.pop().map(WasmValue::V128),
            ValType::F32 => self.pop().map(WasmValue::F32),
            ValType::F64 => self.pop().map(WasmValue::F64),
            ValType::RefExtern => self.pop().map(|v| match v {
                Some(v) => WasmValue::RefExtern(v),
                None => WasmValue::RefNull(ValType::RefExtern),
            }),
            ValType::RefFunc => self.pop().map(|v| match v {
                Some(v) => WasmValue::RefFunc(v),
                None => WasmValue::RefNull(ValType::RefFunc),
            }),
        }
    }

    pub(crate) fn extend_from_wasmvalues(&mut self, values: &[WasmValue]) {
        for value in values.iter() {
            self.push_dyn(value.into())
        }
    }
}

fn truncate_keep<T: Copy + Default>(data: &mut Vec<T>, n: u32, end_keep: u32) {
    let total_to_keep = n + end_keep;
    let len = data.len() as u32;
    assert!(len >= total_to_keep, "total to keep should be less than or equal to self.top");

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
    fn test_truncate_keep() {
        macro_rules! test_macro {
            ($( $n:expr, $end_keep:expr, $expected:expr ),*) => {
            $(
                let mut stack = alloc::vec![1,2,3,4,5];
                truncate_keep(&mut stack, $n, $end_keep);
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
