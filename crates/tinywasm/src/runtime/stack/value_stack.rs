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

#[derive(Debug, Clone, Copy)]
pub(crate) struct StackLocation {
    pub(crate) s32: u32,
    pub(crate) s64: u32,
    pub(crate) s128: u32,
    pub(crate) sref: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StackHeight {
    pub(crate) s32: u32,
    pub(crate) s64: u32,
    pub(crate) s128: u32,
    pub(crate) sref: u32,
}

impl From<ValType> for StackHeight {
    fn from(value: ValType) -> Self {
        match value {
            ValType::I32 | ValType::F32 => Self { s32: 1, ..Default::default() },
            ValType::I64 | ValType::F64 => Self { s64: 1, ..Default::default() },
            ValType::V128 => Self { s128: 1, ..Default::default() },
            ValType::RefExtern | ValType::RefFunc => Self { sref: 1, ..Default::default() },
        }
    }
}

impl From<&[ValType]> for StackHeight {
    fn from(value: &[ValType]) -> Self {
        let mut s32 = 0;
        let mut s64 = 0;
        let mut s128 = 0;
        let mut sref = 0;
        for val_type in value.iter() {
            match val_type {
                ValType::I32 | ValType::F32 => s32 += 1,
                ValType::I64 | ValType::F64 => s64 += 1,
                ValType::V128 => s128 += 1,
                ValType::RefExtern | ValType::RefFunc => sref += 1,
            }
        }
        Self { s32, s64, s128, sref }
    }
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

    pub(crate) fn pop_32(&mut self) -> Result<i32> {
        self.stack_32.pop().ok_or(Error::ValueStackUnderflow).map(|v| v as i32)
    }
    pub(crate) fn pop_64(&mut self) -> Result<i64> {
        self.stack_64.pop().ok_or(Error::ValueStackUnderflow).map(|v| v as i64)
    }
    pub(crate) fn pop_128(&mut self) -> Result<u128> {
        self.stack_128.pop().ok_or(Error::ValueStackUnderflow)
    }
    pub(crate) fn pop_ref(&mut self) -> Result<Option<u32>> {
        self.stack_ref.pop().ok_or(Error::ValueStackUnderflow)
    }
    pub(crate) fn push_32(&mut self, value: i32) {
        self.stack_32.push(value as u32);
    }
    pub(crate) fn push_64(&mut self, value: i64) {
        self.stack_64.push(value as u64);
    }
    pub(crate) fn push_128(&mut self, value: u128) {
        self.stack_128.push(value);
    }
    pub(crate) fn push_ref(&mut self, value: Option<u32>) {
        self.stack_ref.push(value);
    }

    pub(crate) fn drop<const T: u8>(&mut self) -> Result<()> {
        match T {
            0 => self.stack_32.pop().map(|_| ()).ok_or(Error::ValueStackUnderflow),
            1 => self.stack_64.pop().map(|_| ()).ok_or(Error::ValueStackUnderflow),
            2 => self.stack_128.pop().map(|_| ()).ok_or(Error::ValueStackUnderflow),
            3 => self.stack_ref.pop().map(|_| ()).ok_or(Error::ValueStackUnderflow),
            _ => unreachable!("Invalid type"),
        }
    }
    pub(crate) fn select<const T: u8>(&mut self) -> Result<()> {
        macro_rules! select {
            ($($i:literal => $pop:ident, $push:ident),*) => {
                match T {
                    $($i => {
                        let cond = self.pop_32()?;
                        let val2 = self.$pop()?;
                        if cond == 0 {
                            self.drop::<$i>()?;
                            self.$push(val2);
                        }
                    })*
                    _ => unreachable!("Invalid type")
                }
            };
        }
        select!(0 => pop_32, push_32, 1 => pop_64, push_64, 2 => pop_128, push_128, 3 => pop_ref, push_ref);
        Ok(())
    }

    pub(crate) fn pop(&mut self, val_type: ValType) -> Result<WasmValue> {
        match val_type {
            ValType::I32 => self.pop_32().map(WasmValue::I32),
            ValType::I64 => self.pop_64().map(WasmValue::I64),
            ValType::V128 => self.pop_128().map(WasmValue::V128),
            ValType::F32 => self.pop_32().map(|v| WasmValue::F32(f32::from_bits(v as u32))),
            ValType::F64 => self.pop_64().map(|v| WasmValue::F64(f64::from_bits(v as u64))),
            ValType::RefExtern => self.pop_ref().map(|v| match v {
                Some(v) => WasmValue::RefExtern(v),
                None => WasmValue::RefNull(ValType::RefExtern),
            }),
            ValType::RefFunc => self.pop_ref().map(|v| match v {
                Some(v) => WasmValue::RefFunc(v),
                None => WasmValue::RefNull(ValType::RefFunc),
            }),
        }
    }

    pub(crate) fn pop_raw(&mut self, val_type: ValType) -> Result<TinyWasmValue> {
        match val_type {
            ValType::I32 => self.stack_32.pop().ok_or(Error::ValueStackUnderflow).map(TinyWasmValue::Value32),
            ValType::I64 => self.stack_64.pop().ok_or(Error::ValueStackUnderflow).map(TinyWasmValue::Value64),
            ValType::V128 => self.stack_128.pop().ok_or(Error::ValueStackUnderflow).map(TinyWasmValue::Value128),
            ValType::RefExtern => self.stack_ref.pop().ok_or(Error::ValueStackUnderflow).map(TinyWasmValue::ValueRef),
            ValType::RefFunc => self.stack_ref.pop().ok_or(Error::ValueStackUnderflow).map(TinyWasmValue::ValueRef),
            ValType::F32 => self.stack_32.pop().ok_or(Error::ValueStackUnderflow).map(TinyWasmValue::Value32),
            ValType::F64 => self.stack_64.pop().ok_or(Error::ValueStackUnderflow).map(TinyWasmValue::Value64),
        }
    }

    pub(crate) fn pop_many(&mut self, val_types: &[ValType]) -> Result<Vec<WasmValue>> {
        let mut values = Vec::with_capacity(val_types.len());
        for val_type in val_types.iter().rev() {
            values.push(self.pop(*val_type)?);
        }
        Ok(values)
    }

    pub(crate) fn pop_many_raw(&mut self, val_types: &[ValType]) -> Result<Vec<TinyWasmValue>> {
        let mut values = Vec::with_capacity(val_types.len());
        for val_type in val_types.iter().rev() {
            values.push(self.pop_raw(*val_type)?);
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

    pub(crate) fn push(&mut self, value: TinyWasmValue) {
        match value {
            TinyWasmValue::Value32(v) => self.stack_32.push(v),
            TinyWasmValue::Value64(v) => self.stack_64.push(v),
            TinyWasmValue::Value128(v) => self.stack_128.push(v),
            TinyWasmValue::ValueRef(v) => self.stack_ref.push(v),
        }
    }

    pub(crate) fn extend_from_wasmvalues(&mut self, values: &[WasmValue]) {
        for value in values.iter() {
            self.push(value.into())
        }
    }
}
