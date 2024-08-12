use alloc::vec::Vec;
use tinywasm_types::{ValType, ValueCounts, ValueCountsSmall, WasmValue};

use crate::{interpreter::*, Result};

use super::Locals;
pub(crate) const STACK_32_SIZE: usize = 1024 * 32;
pub(crate) const STACK_64_SIZE: usize = 1024 * 16;
pub(crate) const STACK_128_SIZE: usize = 1024 * 8;
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

    #[inline]
    pub(crate) fn peek<T: InternalValue>(&self) -> T {
        T::stack_peek(self)
    }

    #[inline]
    pub(crate) fn pop<T: InternalValue>(&mut self) -> T {
        T::stack_pop(self)
    }

    #[inline]
    pub(crate) fn push<T: InternalValue>(&mut self, value: T) {
        T::stack_push(self, value)
    }

    #[inline]
    pub(crate) fn drop<T: InternalValue>(&mut self) {
        T::stack_pop(self);
    }

    #[inline]
    pub(crate) fn select<T: InternalValue>(&mut self) {
        let cond: i32 = self.pop();
        let val2: T = self.pop();
        if cond == 0 {
            self.drop::<T>();
            self.push(val2);
        }
    }

    #[inline]
    pub(crate) fn calculate_same<T: InternalValue>(&mut self, func: fn(T, T) -> Result<T>) -> Result<()> {
        T::stack_calculate(self, func)
    }

    #[inline]
    pub(crate) fn calculate<T: InternalValue, U: InternalValue>(&mut self, func: fn(T, T) -> Result<U>) -> Result<()> {
        let v2 = T::stack_pop(self);
        let v1 = T::stack_pop(self);
        U::stack_push(self, func(v1, v2)?);
        Ok(())
    }

    #[inline]
    pub(crate) fn replace_top<T: InternalValue, U: InternalValue>(&mut self, func: fn(T) -> Result<U>) -> Result<()> {
        let v1 = T::stack_pop(self);
        U::stack_push(self, func(v1)?);
        Ok(())
    }

    #[inline]
    pub(crate) fn replace_top_same<T: InternalValue>(&mut self, func: fn(T) -> Result<T>) -> Result<()> {
        T::replace_top(self, func)
    }

    pub(crate) fn pop_params(&mut self, val_types: &[ValType]) -> Vec<WasmValue> {
        val_types.iter().map(|val_type| self.pop_wasmvalue(*val_type)).collect::<Vec<_>>()
    }

    pub(crate) fn pop_results(&mut self, val_types: &[ValType]) -> Vec<WasmValue> {
        let mut results = val_types.iter().rev().map(|val_type| self.pop_wasmvalue(*val_type)).collect::<Vec<_>>();
        results.reverse();
        results
    }

    #[inline]
    pub(crate) fn pop_locals(&mut self, pc: ValueCountsSmall, lc: ValueCounts) -> Locals {
        Locals {
            locals_32: {
                let mut locals_32 = { alloc::vec![Value32::default(); lc.c32 as usize].into_boxed_slice() };
                locals_32[0..pc.c32 as usize]
                    .copy_from_slice(&self.stack_32[(self.stack_32.len() - pc.c32 as usize)..]);
                self.stack_32.truncate(self.stack_32.len() - pc.c32 as usize);
                locals_32
            },
            locals_64: {
                let mut locals_64 = { alloc::vec![Value64::default(); lc.c64 as usize].into_boxed_slice() };
                locals_64[0..pc.c64 as usize]
                    .copy_from_slice(&self.stack_64[(self.stack_64.len() - pc.c64 as usize)..]);
                self.stack_64.truncate(self.stack_64.len() - pc.c64 as usize);
                locals_64
            },
            locals_128: {
                let mut locals_128 = { alloc::vec![Value128::default(); lc.c128 as usize].into_boxed_slice() };
                locals_128[0..pc.c128 as usize]
                    .copy_from_slice(&self.stack_128[(self.stack_128.len() - pc.c128 as usize)..]);
                self.stack_128.truncate(self.stack_128.len() - pc.c128 as usize);
                locals_128
            },
            locals_ref: {
                let mut locals_ref = { alloc::vec![ValueRef::default(); lc.cref as usize].into_boxed_slice() };
                locals_ref[0..pc.cref as usize]
                    .copy_from_slice(&self.stack_ref[(self.stack_ref.len() - pc.cref as usize)..]);
                self.stack_ref.truncate(self.stack_ref.len() - pc.cref as usize);
                locals_ref
            },
        }
    }

    pub(crate) fn truncate_keep(&mut self, to: StackLocation, keep: StackHeight) {
        #[inline(always)]
        fn truncate_keep<T: Copy + Default>(data: &mut Vec<T>, n: u32, end_keep: u32) {
            let len = data.len() as u32;
            if len <= n {
                return; // No need to truncate if the current size is already less than or equal to total_to_keep
            }
            data.drain((n as usize)..(len - end_keep) as usize);
        }

        truncate_keep(&mut self.stack_32, to.s32, u32::from(keep.s32));
        truncate_keep(&mut self.stack_64, to.s64, u32::from(keep.s64));
        truncate_keep(&mut self.stack_128, to.s128, u32::from(keep.s128));
        truncate_keep(&mut self.stack_ref, to.sref, u32::from(keep.sref));
    }

    pub(crate) fn push_dyn(&mut self, value: TinyWasmValue) {
        match value {
            TinyWasmValue::Value32(v) => self.stack_32.push(v),
            TinyWasmValue::Value64(v) => self.stack_64.push(v),
            TinyWasmValue::Value128(v) => self.stack_128.push(v),
            TinyWasmValue::ValueRef(v) => self.stack_ref.push(v),
        }
    }

    pub(crate) fn pop_wasmvalue(&mut self, val_type: ValType) -> WasmValue {
        match val_type {
            ValType::I32 => WasmValue::I32(self.pop()),
            ValType::I64 => WasmValue::I64(self.pop()),
            ValType::V128 => WasmValue::V128(self.pop()),
            ValType::F32 => WasmValue::F32(self.pop()),
            ValType::F64 => WasmValue::F64(self.pop()),
            ValType::RefExtern => match self.pop() {
                Some(v) => WasmValue::RefExtern(v),
                None => WasmValue::RefNull(ValType::RefExtern),
            },
            ValType::RefFunc => match self.pop() {
                Some(v) => WasmValue::RefFunc(v),
                None => WasmValue::RefNull(ValType::RefFunc),
            },
        }
    }

    pub(crate) fn extend_from_wasmvalues(&mut self, values: &[WasmValue]) {
        for value in values {
            self.push_dyn(value.into())
        }
    }
}
