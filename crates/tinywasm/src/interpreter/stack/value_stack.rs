use alloc::boxed::Box;
use alloc::vec::Vec;
use tinywasm_types::{ExternRef, FuncRef, LocalAddr, ValType, ValueCounts, ValueCountsSmall, WasmValue};

use crate::{Result, Trap, engine::Config, interpreter::*};

use super::{CallFrame, StackBase};

#[derive(Debug)]
pub(crate) struct ValueStack {
    pub(crate) stack_32: Stack<Value32>,
    pub(crate) stack_64: Stack<Value64>,
    pub(crate) stack_128: Stack<Value128>,
    pub(crate) stack_ref: Stack<ValueRef>,
}

#[derive(Debug)]
pub(crate) struct Stack<T> {
    data: Box<[T]>,
    len: usize,
}

impl<T: Copy + Default> Stack<T> {
    pub(crate) fn with_size(size: usize) -> Self {
        let mut data = Vec::with_capacity(size);
        data.resize_with(size, T::default);
        Self { data: data.into_boxed_slice(), len: 0 }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn push(&mut self, value: T) -> Result<()> {
        if self.len >= self.data.len() {
            return Err(Trap::ValueStackOverflow.into());
        }

        self.data[self.len] = value;
        self.len += 1;
        Ok(())
    }

    pub(crate) fn pop(&mut self) -> T {
        if self.len == 0 {
            unreachable!("ValueStack underflow, this is a bug");
        }

        self.len -= 1;
        self.data[self.len]
    }

    pub(crate) fn last(&self) -> &T {
        if self.len == 0 {
            unreachable!("ValueStack underflow, this is a bug");
        }
        &self.data[self.len - 1]
    }

    pub(crate) fn last_mut(&mut self) -> &mut T {
        if self.len == 0 {
            unreachable!("ValueStack underflow, this is a bug");
        }
        &mut self.data[self.len - 1]
    }

    pub(crate) fn get(&self, index: usize) -> T {
        match self.data.get(index) {
            Some(v) => *v,
            None => unreachable!("Stack index out of bounds, this is a bug"),
        }
    }

    pub(crate) fn set(&mut self, index: usize, value: T) {
        match self.data.get_mut(index) {
            Some(v) => *v = value,
            None => unreachable!("Stack index out of bounds, this is a bug"),
        }
    }

    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        if self.len <= n {
            return;
        }

        let keep_tail = end_keep.min(self.len - n);

        if keep_tail == 0 {
            self.len = n;
            return;
        }

        let tail_start = self.len - keep_tail;
        self.data.copy_within(tail_start..self.len, n);
        self.len = n + keep_tail;
    }

    pub(crate) fn enter_locals(&mut self, param_count: usize, local_count: usize) -> Result<(usize, usize)> {
        debug_assert!(param_count <= local_count, "param count exceeds local count");
        let start =
            self.len.checked_sub(param_count).unwrap_or_else(|| unreachable!("value stack underflow, this is a bug"));

        let end = start + local_count;
        if end > self.data.len() {
            return Err(Trap::ValueStackOverflow.into());
        }

        self.data[(start + param_count)..end].fill(T::default());
        self.len = end;
        Ok((start, end))
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

    pub(crate) fn len(&self) -> usize {
        self.stack_32.len() + self.stack_64.len() + self.stack_128.len() + self.stack_ref.len()
    }

    pub(crate) fn peek<T: InternalValue>(&self) -> T {
        T::stack_peek(self)
    }

    pub(crate) fn pop<T: InternalValue>(&mut self) -> T {
        T::stack_pop(self)
    }

    pub(crate) fn push<T: InternalValue>(&mut self, value: T) -> Result<()> {
        T::stack_push(self, value)
    }

    pub(crate) fn drop<T: InternalValue>(&mut self) {
        T::stack_pop(self);
    }

    pub(crate) fn select<T: InternalValue>(&mut self) -> Result<()> {
        let cond: i32 = self.pop();
        let val2: T = self.pop();
        if cond == 0 {
            self.drop::<T>();
            self.push(val2)?;
        }
        Ok(())
    }

    pub(crate) fn select_multi(&mut self, counts: ValueCountsSmall) {
        let condition = self.pop::<i32>() != 0;
        self.stack_32.select_many(counts.c32 as usize, condition);
        self.stack_64.select_many(counts.c64 as usize, condition);
        self.stack_128.select_many(counts.c128 as usize, condition);
        self.stack_ref.select_many(counts.cref as usize, condition);
    }

    pub(crate) fn binary_same<T: InternalValue>(&mut self, func: impl FnOnce(T, T) -> Result<T>) -> Result<()> {
        T::stack_calculate(self, func)
    }

    pub(crate) fn ternary_same<T: InternalValue>(&mut self, func: impl FnOnce(T, T, T) -> Result<T>) -> Result<()> {
        T::stack_calculate3(self, func)
    }

    pub(crate) fn binary<T: InternalValue, U: InternalValue>(
        &mut self,
        func: impl FnOnce(T, T) -> Result<U>,
    ) -> Result<()> {
        let v2 = T::stack_pop(self);
        let v1 = T::stack_pop(self);
        U::stack_push(self, func(v1, v2)?)?;
        Ok(())
    }

    pub(crate) fn binary_diff<A: InternalValue, B: InternalValue, RES: InternalValue>(
        &mut self,
        func: impl FnOnce(A, B) -> Result<RES>,
    ) -> Result<()> {
        let v2 = B::stack_pop(self);
        let v1 = A::stack_pop(self);
        RES::stack_push(self, func(v1, v2)?)?;
        Ok(())
    }

    pub(crate) fn unary<T: InternalValue, U: InternalValue>(
        &mut self,
        func: impl FnOnce(T) -> Result<U>,
    ) -> Result<()> {
        let v1 = T::stack_pop(self);
        U::stack_push(self, func(v1)?)?;
        Ok(())
    }

    pub(crate) fn unary_same<T: InternalValue>(&mut self, func: impl Fn(T) -> Result<T>) -> Result<()> {
        T::replace_top(self, func)
    }

    pub(crate) fn pop_types<'a>(
        &'a mut self,
        val_types: impl IntoIterator<Item = &'a ValType>,
    ) -> impl core::iter::Iterator<Item = WasmValue> {
        val_types.into_iter().map(|val_type| self.pop_wasmvalue(*val_type))
    }

    pub(crate) fn enter_locals(
        &mut self,
        params: ValueCountsSmall,
        locals: ValueCounts,
    ) -> Result<(StackBase, StackBase, ValueCountsSmall)> {
        let stack_offset = ValueCountsSmall {
            c32: u16::try_from(locals.c32).unwrap_or_else(|_| unreachable!("local count exceeds u16")),
            c64: u16::try_from(locals.c64).unwrap_or_else(|_| unreachable!("local count exceeds u16")),
            c128: u16::try_from(locals.c128).unwrap_or_else(|_| unreachable!("local count exceeds u16")),
            cref: u16::try_from(locals.cref).unwrap_or_else(|_| unreachable!("local count exceeds u16")),
        };

        let (locals_base32, stack_base32) = self.stack_32.enter_locals(params.c32 as usize, locals.c32 as usize)?;
        let (locals_base64, stack_base64) = self.stack_64.enter_locals(params.c64 as usize, locals.c64 as usize)?;
        let (locals_base128, stack_base128) =
            self.stack_128.enter_locals(params.c128 as usize, locals.c128 as usize)?;
        let (locals_baseref, stack_baseref) =
            self.stack_ref.enter_locals(params.cref as usize, locals.cref as usize)?;

        Ok((
            StackBase { s32: locals_base32, s64: locals_base64, s128: locals_base128, sref: locals_baseref },
            StackBase { s32: stack_base32, s64: stack_base64, s128: stack_base128, sref: stack_baseref },
            stack_offset,
        ))
    }

    pub(crate) fn truncate_keep_counts(&mut self, base: StackBase, keep: ValueCountsSmall) {
        self.stack_32.truncate_keep(base.s32, keep.c32 as usize);
        self.stack_64.truncate_keep(base.s64, keep.c64 as usize);
        self.stack_128.truncate_keep(base.s128, keep.c128 as usize);
        self.stack_ref.truncate_keep(base.sref, keep.cref as usize);
    }

    #[inline]
    pub(crate) fn local_get_32(&self, frame: &CallFrame, index: LocalAddr) -> Value32 {
        self.stack_32.get(frame.locals_base.s32 + index as usize)
    }

    #[inline]
    pub(crate) fn local_get_64(&self, frame: &CallFrame, index: LocalAddr) -> Value64 {
        self.stack_64.get(frame.locals_base.s64 + index as usize)
    }

    #[inline]
    pub(crate) fn local_get_128(&self, frame: &CallFrame, index: LocalAddr) -> Value128 {
        self.stack_128.get(frame.locals_base.s128 + index as usize)
    }

    #[inline]
    pub(crate) fn local_get_ref(&self, frame: &CallFrame, index: LocalAddr) -> ValueRef {
        self.stack_ref.get(frame.locals_base.sref + index as usize)
    }

    #[inline]
    pub(crate) fn local_set_32(&mut self, frame: &CallFrame, index: LocalAddr, value: Value32) {
        self.stack_32.set(frame.locals_base.s32 + index as usize, value);
    }

    #[inline]
    pub(crate) fn local_set_64(&mut self, frame: &CallFrame, index: LocalAddr, value: Value64) {
        self.stack_64.set(frame.locals_base.s64 + index as usize, value);
    }

    #[inline]
    pub(crate) fn local_set_128(&mut self, frame: &CallFrame, index: LocalAddr, value: Value128) {
        self.stack_128.set(frame.locals_base.s128 + index as usize, value);
    }

    #[inline]
    pub(crate) fn local_set_ref(&mut self, frame: &CallFrame, index: LocalAddr, value: ValueRef) {
        self.stack_ref.set(frame.locals_base.sref + index as usize, value);
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

    pub(crate) fn pop_wasmvalue(&mut self, val_type: ValType) -> WasmValue {
        match val_type {
            ValType::I32 => WasmValue::I32(self.pop()),
            ValType::I64 => WasmValue::I64(self.pop()),
            ValType::F32 => WasmValue::F32(self.pop()),
            ValType::F64 => WasmValue::F64(self.pop()),
            ValType::RefExtern => WasmValue::RefExtern(ExternRef::new(self.pop())),
            ValType::RefFunc => WasmValue::RefFunc(FuncRef::new(self.pop())),
            ValType::V128 => WasmValue::V128(self.pop::<Value128>().into()),
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
