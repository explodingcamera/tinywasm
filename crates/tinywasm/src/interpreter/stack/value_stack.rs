use alloc::{vec, vec::Vec};
use core::hint::cold_path;
use tinywasm_types::{MemoryArch, ValueCounts, WasmType, WasmValue};

use super::StackBase;
use crate::engine::{Config, StackConfig};
use crate::interpreter::*;
use crate::{Result, Trap};

#[cfg_attr(feature = "debug", derive(Debug))]
/// Physical value lanes used by the interpreter.
///
/// Guest values should normally be accessed through
/// [`InternalValue`] so their stack and global representation stays consistent.
pub(crate) struct ValueStack {
    pub(crate) stack_32: Stack<Value32>,
    pub(crate) stack_64: Stack<Value64>,
    pub(crate) stack_128: Stack<Value128>,
}

struct WasmValues<'a> {
    stack: &'a ValueStack,
    state: &'a crate::store::State,
    types: core::slice::Iter<'a, WasmType>,
    index: StackBase,
    pin_refs: bool,
}

impl Iterator for WasmValues<'_> {
    type Item = WasmValue;

    fn next(&mut self) -> Option<Self::Item> {
        let value = match *self.types.next()? {
            WasmType::I32 => {
                let value = *self.stack.stack_32.get(self.index.s32 as usize) as i32;
                self.index.s32 += 1;
                WasmValue::I32(value)
            }
            WasmType::I64 => {
                let value = *self.stack.stack_64.get(self.index.s64 as usize) as i64;
                self.index.s64 += 1;
                WasmValue::I64(value)
            }
            WasmType::F32 => {
                let value = f32::from_bits(*self.stack.stack_32.get(self.index.s32 as usize));
                self.index.s32 += 1;
                WasmValue::F32(value)
            }
            WasmType::F64 => {
                let value = f64::from_bits(*self.stack.stack_64.get(self.index.s64 as usize));
                self.index.s64 += 1;
                WasmValue::F64(value)
            }
            WasmType::Ref(ty) => {
                let value =
                    self.state.to_ref_value(ValueRef::from_raw(*self.stack.stack_32.get(self.index.s32 as usize)), ty);
                self.index.s32 += 1;
                WasmValue::Ref(value)
            }
            WasmType::V128 => {
                let value = self.stack.stack_128.get(self.index.s128 as usize).0;
                self.index.s128 += 1;
                WasmValue::V128(value)
            }
        };
        if self.pin_refs
            && let WasmValue::Ref(value) = value
        {
            self.state.pin_host_ref(value);
        }
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.types.size_hint()
    }
}

impl ExactSizeIterator for WasmValues<'_> {}

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct Stack<T: Copy + Default> {
    data: Vec<T>,
    max_size: usize,
    dynamic: bool,
}

impl<T: Copy + Default> Stack<T> {
    pub(crate) fn new(config: StackConfig) -> Self {
        Self { data: Vec::with_capacity(config.initial_size), max_size: config.max_size, dynamic: config.dynamic }
    }

    pub(crate) fn clear(&mut self) {
        self.data.clear();
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, value: T) -> Result<(), Trap> {
        if !self.ensure_capacity_for(self.data.len() + 1) {
            return cold!(Err(Trap::ValueStackOverflow));
        }

        self.data.push(value);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn push_copy(&mut self, index: usize) -> Result<(), Trap> {
        if !self.ensure_capacity_for(self.data.len() + 1) {
            return cold!(Err(Trap::ValueStackOverflow));
        }

        let value = self.data[index];
        self.data.push(value);
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn pop(&mut self) -> T {
        self.data.pop().unwrap_or_else(|| unreachable!("ValueStack underflow, this is a bug"))
    }

    #[inline(always)]
    pub(crate) fn last(&self) -> &T {
        self.data.last().unwrap_or_else(|| unreachable!("ValueStack underflow, this is a bug"))
    }

    #[inline(always)]
    pub(crate) fn get(&self, index: usize) -> &T {
        &self.data[index]
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, index: usize, value: T) {
        self.data[index] = value;
    }

    #[inline(always)]
    pub(crate) fn copy(&mut self, from: usize, to: usize) {
        self.data[to] = self.data[from];
    }

    #[inline(always)]
    pub(crate) fn truncate_keep(&mut self, n: usize, end_keep: usize) {
        let len = self.data.len();
        debug_assert!(n <= len);
        if n >= len {
            return;
        }

        let keep = (len - n).min(end_keep);
        self.data.copy_within(len - keep..len, n);
        self.data.truncate(n + keep);
    }

    #[inline(always)]
    pub(crate) fn truncate_to(&mut self, n: usize) {
        debug_assert!(n <= self.data.len());
        self.data.truncate(n);
    }

    #[inline(always)]
    pub(crate) fn truncate_to_one_tail(&mut self, n: usize) {
        debug_assert!(n < self.data.len());
        let last = self.data.pop().unwrap_or_else(|| unreachable!("ValueStack underflow, this is a bug"));
        self.data.truncate(n);
        self.data.push(last);
    }

    #[inline(always)]
    pub(crate) fn enter_locals(&mut self, param_count: usize, local_count: usize) -> Result<u32, Trap> {
        debug_assert!(param_count <= local_count);
        debug_assert!(param_count <= self.data.len());

        let len = self.data.len();
        let start = len - param_count;
        let end = start + local_count;

        if end > self.data.capacity() {
            cold_path();
            if end > self.max_size || !self.dynamic {
                return Err(Trap::ValueStackOverflow);
            }
            let cap = self.data.capacity();
            let target = end.max(cap.max(1).saturating_mul(2)).min(self.max_size);
            if self.data.try_reserve_exact(target - len).is_err() {
                return Err(Trap::ValueStackOverflow);
            }
        }

        self.data.resize(end, T::default());
        Ok(start as u32)
    }

    fn ensure_capacity_for(&mut self, required_len: usize) -> bool {
        let cap = self.data.capacity();

        if required_len > cap {
            cold_path();

            if required_len > self.max_size || !self.dynamic {
                return false;
            }
            let doubled = cap.max(1).saturating_mul(2);
            let target = required_len.max(doubled).min(self.max_size);
            let additional = target - cap;
            if self.data.try_reserve_exact(additional).is_err() {
                return false;
            }
        }

        true
    }

    #[inline(always)]
    pub(crate) fn select_many(&mut self, count: usize, condition: bool) {
        if count == 0 {
            return;
        }

        let len = self.data.len();
        let needed = count.checked_mul(2).unwrap_or_else(|| unreachable!("Stack underflow, this is a bug"));

        if len < needed {
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

impl<'a, T: Copy + Default> IntoIterator for &'a Stack<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}
impl ValueStack {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            stack_32: Stack::new(config.value_stack_32),
            stack_64: Stack::new(config.value_stack_64),
            stack_128: Stack::new(config.value_stack_128),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.stack_32.clear();
        self.stack_64.clear();
        self.stack_128.clear();
    }

    #[inline(always)]
    pub(crate) fn base(&self) -> StackBase {
        StackBase {
            s32: self.stack_32.len() as u32,
            s64: self.stack_64.len() as u32,
            s128: self.stack_128.len() as u32,
        }
    }

    pub(crate) fn base_before(&self, counts: ValueCounts) -> StackBase {
        let base = self.base();
        StackBase {
            s32: base.s32 - counts.c32 as u32,
            s64: base.s64 - counts.c64 as u32,
            s128: base.s128 - counts.c128 as u32,
        }
    }

    #[inline(always)]
    pub(crate) fn len(&self) -> usize {
        self.stack_32.len() + self.stack_64.len() + self.stack_128.len()
    }

    #[inline(always)]
    pub(crate) fn push<T: InternalValue>(&mut self, value: T) -> Result<(), Trap> {
        T::stack_push(self, value)
    }

    #[inline(always)]
    pub(crate) fn pop_memory_operand(&mut self, arch: MemoryArch) -> Result<usize, Trap> {
        match arch {
            MemoryArch::I32 => Ok(self.stack_32.pop() as usize),
            MemoryArch::I64 => {
                let value = self.stack_64.pop();
                #[cfg(target_pointer_width = "64")]
                return Ok(value as usize);
                #[cfg(not(target_pointer_width = "64"))]
                return cold_err!(usize::try_from(value).map_err(|_| Trap::MemoryOutOfBounds {
                    offset: usize::MAX,
                    len: 0,
                    max: usize::MAX,
                }));
            }
        }
    }

    #[inline]
    pub(crate) fn select_multi(&mut self, counts: ValueCounts) {
        let condition = i32::stack_pop(self) != 0;
        self.stack_32.select_many(counts.c32 as usize, condition);
        self.stack_64.select_many(counts.c64 as usize, condition);
        self.stack_128.select_many(counts.c128 as usize, condition);
    }

    #[inline(always)]
    pub(crate) fn enter_locals(&mut self, params: &ValueCounts, locals: &ValueCounts) -> Result<StackBase, Trap> {
        let locals_base32 = self.stack_32.enter_locals(params.c32 as usize, locals.c32 as usize)?;
        let locals_base64 = self.stack_64.enter_locals(params.c64 as usize, locals.c64 as usize)?;
        let locals_base128 = self.stack_128.enter_locals(params.c128 as usize, locals.c128 as usize)?;
        Ok(StackBase { s32: locals_base32, s64: locals_base64, s128: locals_base128 })
    }

    #[inline]
    /// Pushes call arguments and allocates the function's local lanes.
    pub(crate) fn enter_wasm_call(
        &mut self,
        values: &[WasmValue],
        params: ValueCounts,
        locals: ValueCounts,
        base: StackBase,
    ) -> Result<StackBase, Trap> {
        self.extend_wasmvalues(values.iter().copied()).inspect_err(|_| self.truncate_to_base(base))?;
        self.enter_locals(&params, &locals).inspect_err(|_| self.truncate_to_base(base))
    }

    #[inline(always)]
    pub(crate) fn truncate_keep_counts(&mut self, base: StackBase, keep: ValueCounts) {
        self.stack_32.truncate_keep(base.s32 as usize, keep.c32 as usize);
        self.stack_64.truncate_keep(base.s64 as usize, keep.c64 as usize);
        self.stack_128.truncate_keep(base.s128 as usize, keep.c128 as usize);
    }

    #[inline(always)]
    pub(crate) fn truncate_to_base(&mut self, base: StackBase) {
        self.stack_32.truncate_to(base.s32 as usize);
        self.stack_64.truncate_to(base.s64 as usize);
        self.stack_128.truncate_to(base.s128 as usize);
    }

    pub(crate) fn push_dyn(&mut self, value: TinyWasmValue) -> Result<(), Trap> {
        match value {
            TinyWasmValue::Value32(v) => self.stack_32.push(v),
            TinyWasmValue::Value64(v) => self.stack_64.push(v),
            TinyWasmValue::Value128(v) => self.stack_128.push(v),
            TinyWasmValue::ValueRef(v) => self.stack_32.push(v.raw()),
        }
    }

    pub(crate) fn pop_wasmvalue(&mut self, state: &crate::store::State, val_type: WasmType) -> WasmValue {
        match val_type {
            WasmType::I32 => WasmValue::I32(self.stack_32.pop() as i32),
            WasmType::I64 => WasmValue::I64(self.stack_64.pop() as i64),
            WasmType::F32 => WasmValue::F32(f32::from_bits(self.stack_32.pop())),
            WasmType::F64 => WasmValue::F64(f64::from_bits(self.stack_64.pop())),
            WasmType::Ref(ty) => WasmValue::Ref(state.to_ref_value(ValueRef::from_raw(self.stack_32.pop()), ty)),
            WasmType::V128 => WasmValue::V128(self.stack_128.pop().0),
        }
    }

    /// Pops values in their logical WebAssembly order.
    pub(crate) fn pop_wasmvalues(&mut self, state: &crate::store::State, types: &[WasmType]) -> Vec<WasmValue> {
        debug_assert!(self.len() >= types.len());
        let mut values = vec![WasmValue::I32(0); types.len()];
        for (index, &ty) in types.iter().enumerate().rev() {
            values[index] = self.pop_wasmvalue(state, ty);
        }
        values
    }

    pub(crate) fn wasm_values<'a>(
        &'a self,
        state: &'a crate::store::State,
        types: &'a [WasmType],
        index: StackBase,
        pin_refs: bool,
    ) -> impl ExactSizeIterator<Item = WasmValue> + 'a {
        WasmValues { stack: self, state, types: types.iter(), index, pin_refs }
    }

    pub(crate) fn extend_wasmvalues(&mut self, values: impl Iterator<Item = WasmValue>) -> Result<(), Trap> {
        for value in values {
            match value {
                WasmValue::I32(v) => self.stack_32.push(v as u32)?,
                WasmValue::I64(v) => self.stack_64.push(v as u64)?,
                WasmValue::F32(v) => self.stack_32.push(v.to_bits())?,
                WasmValue::F64(v) => self.stack_64.push(v.to_bits())?,
                WasmValue::Ref(v) => self.stack_32.push(ValueRef::from(v).raw())?,
                WasmValue::V128(v) => self.stack_128.push(v.into())?,
            }
        }
        Ok(())
    }
}
