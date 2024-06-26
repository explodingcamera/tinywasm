use alloc::vec::Vec;
use tinywasm_types::{ValType, WasmValue};

use super::values::*;
use crate::{Error, Result};
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

    pub(crate) fn pop<T: StackValue>(&mut self) -> Result<T> {
        T::pop(self)
    }

    pub(crate) fn push<T: StackValue>(&mut self, value: T) {
        T::push(self, value)
    }

    pub(crate) fn drop<T: StackValue>(&mut self) -> Result<()> {
        T::pop(self).map(|_| ())
    }

    pub(crate) fn select<T: StackValue>(&mut self) -> Result<()> {
        let cond: i32 = self.pop()?;
        let val2: T = self.pop()?;
        if cond == 0 {
            self.drop::<T>()?;
            self.push(val2);
        }
        Ok(())
    }

    pub(crate) fn calculate<T: StackValue, U: StackValue>(&mut self, func: fn(T, T) -> Result<U>) -> Result<()> {
        let v2 = T::pop(self)?;
        let v1 = T::pop(self)?;
        U::push(self, func(v1, v2)?);
        Ok(())
    }

    pub(crate) fn replace_top<T: StackValue, U: StackValue>(&mut self, func: fn(T) -> Result<U>) -> Result<()> {
        let v1 = T::pop(self)?;
        U::push(self, func(v1)?);
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

    pub(crate) fn truncate(&mut self, height: &StackLocation) {
        self.stack_32.truncate(height.s32 as usize);
        self.stack_64.truncate(height.s64 as usize);
        self.stack_128.truncate(height.s128 as usize);
        self.stack_ref.truncate(height.sref as usize);
    }

    pub(crate) fn truncate_keep(&mut self, height: &StackLocation, keep: &StackHeight) {
        self.stack_32.drain(height.s32 as usize..(self.stack_128.len() - keep.s32 as usize));
        self.stack_64.drain(height.s64 as usize..(self.stack_64.len() - keep.s64 as usize));
        self.stack_128.drain(height.s128 as usize..(self.stack_128.len() - keep.s128 as usize));
        self.stack_ref.drain(height.sref as usize..(self.stack_ref.len() - keep.sref as usize));
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

mod sealed {
    #[allow(unreachable_pub)]
    pub trait Sealed {}
}

impl sealed::Sealed for i32 {}
impl sealed::Sealed for f32 {}
impl sealed::Sealed for i64 {}
impl sealed::Sealed for u64 {}
impl sealed::Sealed for f64 {}
impl sealed::Sealed for u32 {}
impl sealed::Sealed for Value128 {}
impl sealed::Sealed for ValueRef {}

pub(crate) trait StackValue: sealed::Sealed {
    fn push(stack: &mut ValueStack, value: Self);
    fn pop(stack: &mut ValueStack) -> Result<Self>
    where
        Self: Sized;
}

impl StackValue for i32 {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_32.push(value as u32);
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_32.pop().ok_or(Error::ValueStackUnderflow).map(|v| v as i32)
    }
}

impl StackValue for f32 {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_32.push(value.to_bits());
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_32.pop().ok_or(Error::ValueStackUnderflow).map(f32::from_bits)
    }
}

impl StackValue for i64 {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_64.push(value as u64);
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_64.pop().ok_or(Error::ValueStackUnderflow).map(|v| v as i64)
    }
}

impl StackValue for u64 {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_64.push(value);
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_64.pop().ok_or(Error::ValueStackUnderflow)
    }
}

impl StackValue for f64 {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_64.push(value.to_bits());
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_64.pop().ok_or(Error::ValueStackUnderflow).map(f64::from_bits)
    }
}

impl StackValue for u32 {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_32.push(value);
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_32.pop().ok_or(Error::ValueStackUnderflow)
    }
}

impl StackValue for Value128 {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_128.push(value);
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_128.pop().ok_or(Error::ValueStackUnderflow)
    }
}

impl StackValue for ValueRef {
    #[inline]
    fn push(stack: &mut ValueStack, value: Self) {
        stack.stack_ref.push(value);
    }
    #[inline]
    fn pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_ref.pop().ok_or(Error::ValueStackUnderflow)
    }
}
