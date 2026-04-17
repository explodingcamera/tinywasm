use alloc::vec::Vec;
use core::hint::cold_path;
use tinywasm_types::{ExternRef, FuncRef, LocalAddr, ValueCounts, WasmType, WasmValue};

use super::{CallFrame, StackBase};
use crate::{Result, Trap, engine::Config, interpreter::*};

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct ValueStack {
    pub(crate) stack_32: Stack<Value32>,
    pub(crate) stack_64: Stack<Value64>,
    pub(crate) stack_128: Stack<Value128>,
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct Stack<T: Copy + Default> {
    data: Vec<T>,
}

impl<T: Copy + Default> Stack<T> {
    pub(crate) fn new(size: usize) -> Self {
        Self { data: Vec::with_capacity(size) }
    }

    pub(crate) fn clear(&mut self) {
        self.data.clear();
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) -> Result<()> {
        if self.data.len() == self.data.capacity() {
            cold_path();
            return Err(Trap::ValueStackOverflow.into());
        }

        self.data.push(value);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> T {
        self.data.pop().unwrap_or_else(|| {
            cold_path();
            unreachable!("ValueStack underflow, this is a bug");
        })
    }

    #[inline(always)]
    pub(crate) fn last(&self) -> &T {
        self.data.last().unwrap_or_else(|| {
            cold_path();
            unreachable!("ValueStack underflow, this is a bug");
        })
    }

    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> &T {
        self.data.get(index).unwrap_or_else(|| {
            cold_path();
            unreachable!("Stack index out of bounds, this is a bug");
        })
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, index: usize, value: T) {
        *self.data.get_mut(index).unwrap_or_else(|| {
            cold_path();
            unreachable!("Stack index out of bounds, this is a bug");
        }) = value;
    }

    #[inline(always)]
    pub(crate) fn get_mut(&mut self, index: usize) -> &mut T {
        self.data.get_mut(index).unwrap_or_else(|| {
            cold_path();
            unreachable!("Stack index out of bounds, this is a bug");
        })
    }

    #[inline(always)]
    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        let len = self.data.len();
        debug_assert!(n <= len);
        if n >= len {
            return;
        }

        let keep = (len - n).min(end_keep);
        if keep > 0 {
            self.data.copy_within(len - keep..len, n);
        }
        self.data.truncate(n + keep);
    }

    #[inline(always)]
    pub(crate) fn enter_locals(&mut self, param_count: usize, local_count: usize) -> Result<u32> {
        debug_assert!(param_count <= local_count && param_count <= self.data.len());

        let start = self.data.len() - param_count;
        let end = start + local_count;
        if end > self.data.capacity() {
            cold_path();
            return Err(Trap::ValueStackOverflow.into());
        }

        self.data.resize(end, T::default());
        Ok(start as u32)
    }

    #[inline(always)]
    pub(crate) fn select_many(&mut self, count: usize, condition: bool) {
        if count == 0 {
            return;
        }

        let len = self.data.len();
        let needed = count.checked_mul(2).unwrap_or_else(|| {
            cold_path();
            unreachable!("Stack underflow, this is a bug");
        });

        if len < needed {
            cold_path();
            unreachable!("Stack underflow, this is a bug");
        }

        if !condition {
            let dst = len - needed;
            let src = len - count;
            self.data.copy_within(src..len, dst);
        }

        self.data.truncate(len - count);
    }
}
impl ValueStack {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            stack_32: Stack::new(config.stack_32_size),
            stack_64: Stack::new(config.stack_64_size),
            stack_128: Stack::new(config.stack_128_size),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.stack_32.clear();
        self.stack_64.clear();
        self.stack_128.clear();
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.stack_32.len() + self.stack_64.len() + self.stack_128.len()
    }

    #[inline(always)]
    pub(crate) fn peek<T: InternalValue>(&self) -> T {
        T::stack_peek(self)
    }

    #[inline(always)]
    pub(crate) fn pop<T: InternalValue>(&mut self) -> T {
        T::stack_pop(self)
    }

    #[inline(always)]
    pub(crate) fn push<T: InternalValue>(&mut self, value: T) -> Result<()> {
        T::stack_push(self, value)
    }

    #[inline(always)]
    pub(crate) fn drop<T: InternalValue>(&mut self) {
        T::stack_pop(self);
    }

    #[inline(always)]
    pub(crate) fn select<T: InternalValue>(&mut self) -> Result<()> {
        let cond: i32 = self.pop();
        let val2: T = self.pop();
        if cond == 0 {
            self.drop::<T>();
            self.push(val2)?;
        }
        Ok(())
    }

    #[inline]
    pub(crate) fn select_multi(&mut self, counts: ValueCounts) {
        let condition = self.pop::<i32>() != 0;
        self.stack_32.select_many(counts.c32 as usize, condition);
        self.stack_64.select_many(counts.c64 as usize, condition);
        self.stack_128.select_many(counts.c128 as usize, condition);
    }

    pub(crate) fn pop_types<'a>(
        &'a mut self,
        val_types: impl IntoIterator<Item = &'a WasmType>,
    ) -> impl core::iter::Iterator<Item = WasmValue> {
        val_types.into_iter().map(|val_type| self.pop_wasmvalue(*val_type))
    }

    pub(crate) fn enter_locals(&mut self, params: &ValueCounts, locals: &ValueCounts) -> Result<StackBase> {
        let locals_base32 = self.stack_32.enter_locals(params.c32 as usize, locals.c32 as usize)?;
        let locals_base64 = self.stack_64.enter_locals(params.c64 as usize, locals.c64 as usize)?;
        let locals_base128 = self.stack_128.enter_locals(params.c128 as usize, locals.c128 as usize)?;
        Ok(StackBase { s32: locals_base32, s64: locals_base64, s128: locals_base128 })
    }

    pub(crate) fn truncate_keep_counts(&mut self, base: StackBase, keep: ValueCounts) {
        self.stack_32.truncate_keep(base.s32 as usize, keep.c32 as usize);
        self.stack_64.truncate_keep(base.s64 as usize, keep.c64 as usize);
        self.stack_128.truncate_keep(base.s128 as usize, keep.c128 as usize);
    }

    #[inline]
    pub(crate) fn local_get<T: InternalValue>(&self, frame: &CallFrame, index: LocalAddr) -> T {
        T::local_get(self, frame, index)
    }

    #[inline]
    pub(crate) fn local_update<T: InternalValue>(
        &mut self,
        frame: &CallFrame,
        index: LocalAddr,
        func: impl FnOnce(T) -> T,
    ) {
        T::local_update(self, frame, index, func)
    }

    #[inline]
    pub(crate) fn local_set<T: InternalValue>(&mut self, frame: &CallFrame, index: LocalAddr, value: T) {
        T::local_set(self, frame, index, value);
    }

    pub(crate) fn push_dyn(&mut self, value: TinyWasmValue) -> Result<()> {
        match value {
            TinyWasmValue::Value32(v) => self.stack_32.push(v)?,
            TinyWasmValue::Value64(v) => self.stack_64.push(v)?,
            TinyWasmValue::Value128(v) => self.stack_128.push(v)?,
            TinyWasmValue::ValueRef(v) => self.stack_32.push(v.raw())?,
        }
        Ok(())
    }

    pub(crate) fn pop_wasmvalue(&mut self, val_type: WasmType) -> WasmValue {
        match val_type {
            WasmType::I32 => WasmValue::I32(self.pop()),
            WasmType::I64 => WasmValue::I64(self.pop()),
            WasmType::F32 => WasmValue::F32(self.pop()),
            WasmType::F64 => WasmValue::F64(self.pop()),
            WasmType::RefExtern => WasmValue::RefExtern(ExternRef::from_raw(self.pop::<ValueRef>().raw())),
            WasmType::RefFunc => WasmValue::RefFunc(FuncRef::from_raw(self.pop::<ValueRef>().raw())),
            WasmType::V128 => WasmValue::V128(self.pop::<Value128>().into()),
        }
    }

    pub(crate) fn extend_from_wasmvalues(&mut self, values: &[WasmValue]) -> Result<()> {
        for value in values {
            match value {
                WasmValue::I32(v) => self.stack_32.push(*v as u32)?,
                WasmValue::I64(v) => self.stack_64.push(*v as u64)?,
                WasmValue::F32(v) => self.stack_32.push(v.to_bits())?,
                WasmValue::F64(v) => self.stack_64.push(v.to_bits())?,
                WasmValue::RefExtern(v) => self.stack_32.push(v.raw())?,
                WasmValue::RefFunc(v) => self.stack_32.push(v.raw())?,
                WasmValue::V128(v) => self.stack_128.push((*v).into())?,
            }
        }
        Ok(())
    }
}
