use alloc::boxed::Box;
use alloc::vec::Vec;
use tinywasm_types::{ExternRef, FuncRef, LocalAddr, ValueCounts, WasmType, WasmValue};

use crate::{Result, Trap, engine::Config, interpreter::*, unlikely};

use super::{CallFrame, StackBase};

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct ValueStack {
    pub(crate) stack_32: Stack<Value32>,
    pub(crate) stack_64: Stack<Value64>,
    pub(crate) stack_128: Stack<Value128>,
    pub(crate) stack_ref: Stack<ValueRef>,
}

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct Stack<T: Copy + Default> {
    data: Box<[T]>,
    len: usize,
}

impl<T: Copy + Default> Stack<T> {
    pub(crate) fn with_size(size: usize) -> Self {
        let mut data = Vec::with_capacity(size);
        data.resize_with(size, T::default);
        Self { data: data.into_boxed_slice(), len: 0 }
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) -> Result<()> {
        if unlikely(self.len >= self.data.len()) {
            return Err(Trap::ValueStackOverflow.into());
        }
        self.data[self.len] = value;
        self.len += 1;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> T {
        if self.len == 0 {
            unreachable!("ValueStack underflow, this is a bug");
        }
        self.len -= 1;
        self.data[self.len]
    }

    #[inline(always)]
    pub(crate) fn last(&self) -> &T {
        if self.len == 0 {
            unreachable!("ValueStack underflow, this is a bug");
        }
        &self.data[self.len - 1]
    }

    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> T {
        match self.data.get(index) {
            Some(v) => *v,
            None => unreachable!("Stack index out of bounds, this is a bug"),
        }
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, index: usize, value: T) {
        match self.data.get_mut(index) {
            Some(v) => *v = value,
            None => unreachable!("Stack index out of bounds, this is a bug"),
        }
    }

    #[inline(always)]
    pub(crate) fn get_mut(&mut self, index: usize) -> &mut T {
        match self.data.get_mut(index) {
            Some(v) => v,
            None => unreachable!("Stack index out of bounds, this is a bug"),
        }
    }

    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        debug_assert!(n <= self.len);
        let len = self.len;
        if n >= len {
            return;
        }

        if end_keep == 0 {
            self.len = n;
            return;
        }

        let keep = (len - n).min(end_keep);
        self.data.copy_within((len - keep)..len, n);
        self.len = n + keep;
    }

    pub(crate) fn enter_locals(&mut self, param_count: usize, local_count: usize) -> Result<u32> {
        debug_assert!(param_count <= local_count);
        let start = self.len - param_count;
        let end = start + local_count;

        if unlikely(end > self.data.len()) {
            return Err(Trap::ValueStackOverflow.into());
        }

        let init_start = start + param_count;
        if init_start != end {
            self.data[init_start..end].fill(T::default());
        }
        self.len = end;
        Ok(start as u32)
    }

    pub(crate) fn select_many(&mut self, count: usize, condition: bool) {
        if count == 0 {
            return;
        }
        if self.len < count * 2 {
            unreachable!("Stack underflow, this is a bug");
        }

        if !condition {
            let start = self.len - (count * 2);
            let second_start = self.len - count;
            self.data.copy_within(second_start..self.len, start);
        }
        self.len -= count;
    }
}

impl ValueStack {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            stack_32: Stack::with_size(config.stack_32_size),
            stack_64: Stack::with_size(config.stack_64_size),
            stack_128: Stack::with_size(config.stack_128_size),
            stack_ref: Stack::with_size(config.stack_ref_size),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.stack_32.clear();
        self.stack_64.clear();
        self.stack_128.clear();
        self.stack_ref.clear();
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.stack_32.len + self.stack_64.len + self.stack_128.len + self.stack_ref.len
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
        self.stack_ref.select_many(counts.cref as usize, condition);
    }

    pub(crate) fn pop_types<'a>(
        &'a mut self,
        val_types: impl IntoIterator<Item = &'a WasmType>,
    ) -> impl core::iter::Iterator<Item = WasmValue> {
        val_types.into_iter().map(|val_type| self.pop_wasmvalue(*val_type))
    }

    pub(crate) fn enter_locals(&mut self, params: &ValueCounts, locals: &ValueCounts) -> Result<StackBase> {
        let locals_base32 = if params.c32 == 0 && locals.c32 == 0 {
            self.stack_32.len as u32
        } else {
            self.stack_32.enter_locals(params.c32 as usize, locals.c32 as usize)?
        };
        let locals_base64 = if params.c64 == 0 && locals.c64 == 0 {
            self.stack_64.len as u32
        } else {
            self.stack_64.enter_locals(params.c64 as usize, locals.c64 as usize)?
        };
        let locals_base128 = if params.c128 == 0 && locals.c128 == 0 {
            self.stack_128.len as u32
        } else {
            self.stack_128.enter_locals(params.c128 as usize, locals.c128 as usize)?
        };
        let locals_baseref = if params.cref == 0 && locals.cref == 0 {
            self.stack_ref.len as u32
        } else {
            self.stack_ref.enter_locals(params.cref as usize, locals.cref as usize)?
        };

        Ok(StackBase { s32: locals_base32, s64: locals_base64, s128: locals_base128, sref: locals_baseref })
    }

    pub(crate) fn truncate_keep_counts(&mut self, base: StackBase, keep: ValueCounts) {
        if keep.is_empty() {
            self.stack_32.len = base.s32 as usize;
            self.stack_64.len = base.s64 as usize;
            self.stack_128.len = base.s128 as usize;
            self.stack_ref.len = base.sref as usize;
            return;
        }

        self.stack_32.truncate_keep(base.s32 as usize, keep.c32 as usize);
        self.stack_64.truncate_keep(base.s64 as usize, keep.c64 as usize);
        self.stack_128.truncate_keep(base.s128 as usize, keep.c128 as usize);
        self.stack_ref.truncate_keep(base.sref as usize, keep.cref as usize);
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
        func: impl FnOnce(&mut T),
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
            TinyWasmValue::ValueRef(v) => self.stack_ref.push(v)?,
        }
        Ok(())
    }

    pub(crate) fn pop_wasmvalue(&mut self, val_type: WasmType) -> WasmValue {
        match val_type {
            WasmType::I32 => WasmValue::I32(self.pop()),
            WasmType::I64 => WasmValue::I64(self.pop()),
            WasmType::F32 => WasmValue::F32(self.pop()),
            WasmType::F64 => WasmValue::F64(self.pop()),
            WasmType::RefExtern => WasmValue::RefExtern(ExternRef::new(self.pop())),
            WasmType::RefFunc => WasmValue::RefFunc(FuncRef::new(self.pop())),
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
                WasmValue::RefExtern(v) => self.stack_ref.push(v.addr())?,
                WasmValue::RefFunc(v) => self.stack_ref.push(v.addr())?,
                WasmValue::V128(v) => self.stack_128.push((*v).into())?,
            }
        }
        Ok(())
    }
}
