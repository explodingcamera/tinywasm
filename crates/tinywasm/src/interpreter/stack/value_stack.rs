use tinywasm_types::{MemoryArch, ValueCounts};

use super::StackBase;
use crate::collections::StackLane;
use crate::engine::Config;
use crate::interpreter::*;
use crate::{Result, Trap};

#[derive(Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
/// Physical value lanes used by the interpreter.
///
/// Guest values should normally be accessed through
/// [`InternalValue`] so their stack and global representation stays consistent.
pub(crate) struct ValueStack {
    pub(crate) stack_32: StackLane<Value32>,
    pub(crate) stack_64: StackLane<Value64>,
    pub(crate) stack_128: StackLane<Value128>,
}

impl ValueStack {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            stack_32: StackLane::new(config.value_stack_32),
            stack_64: StackLane::new(config.value_stack_64),
            stack_128: StackLane::new(config.value_stack_128),
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
    pub(crate) fn pop_memory_operand(&mut self, arch: MemoryArch) -> Result<usize, Trap> {
        match arch {
            MemoryArch::I32 => Ok(u32::stack_pop(self) as usize),
            MemoryArch::I64 => {
                let value = u64::stack_pop(self);
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

    /// Reserves all lanes before zeroing locals or publishing any new live height.
    #[inline(always)]
    pub(crate) fn enter_locals(
        &mut self,
        params: &ValueCounts,
        locals: &ValueCounts,
        max_stack: &ValueCounts,
    ) -> Result<StackBase, Trap> {
        let base = self.base_before(*params);
        let end = self.prepare_locals(base, locals, max_stack)?;
        self.initialize_locals(end);
        Ok(base)
    }

    /// Prepares a tail call's final layout before moving arguments out of the current frame.
    #[inline]
    pub(crate) fn replace_locals(
        &mut self,
        base: StackBase,
        params: &ValueCounts,
        locals: &ValueCounts,
        max_stack: &ValueCounts,
    ) -> Result<StackBase, Trap> {
        let end = self.prepare_locals(base, locals, max_stack)?;
        self.truncate_keep_counts(base, *params);
        self.initialize_locals(end);
        Ok(base)
    }

    #[inline(always)]
    fn prepare_locals(
        &mut self,
        base: StackBase,
        locals: &ValueCounts,
        max_stack: &ValueCounts,
    ) -> Result<StackBase, Trap> {
        let s32 = self.stack_32.prepare_locals(base.s32 as usize, locals.c32 as usize, max_stack.c32 as usize)?;
        let s64 = self.stack_64.prepare_locals(base.s64 as usize, locals.c64 as usize, max_stack.c64 as usize)?;
        let s128 = self.stack_128.prepare_locals(base.s128 as usize, locals.c128 as usize, max_stack.c128 as usize)?;
        // Each lane checked the reservation, including these frame-base conversions, before growth.
        Ok(StackBase { s32: s32 as u32, s64: s64 as u32, s128: s128 as u32 })
    }

    #[inline]
    fn initialize_locals(&mut self, end: StackBase) {
        self.stack_32.initialize_locals(end.s32 as usize);
        self.stack_64.initialize_locals(end.s64 as usize);
        self.stack_128.initialize_locals(end.s128 as usize);
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

    /// Pushes a dynamically typed value inside a function body using its entry reservation.
    pub(crate) fn push_reserved(&mut self, value: RuntimeValue) {
        match value {
            RuntimeValue::Value32(value) => self.stack_32.push(value),
            RuntimeValue::Value64(value) => self.stack_64.push(value),
            RuntimeValue::Value128(value) => self.stack_128.push(value),
            RuntimeValue::ValueRef(value) => self.stack_32.push(value.raw()),
        }
    }

    /// Pushes a value from outside a function body's reservation, see [`StackLane::push_or_grow`].
    pub(crate) fn push_dyn(&mut self, value: RuntimeValue) -> Result<(), Trap> {
        match value {
            RuntimeValue::Value32(value) => self.stack_32.push_or_grow(value),
            RuntimeValue::Value64(value) => self.stack_64.push_or_grow(value),
            RuntimeValue::Value128(value) => self.stack_128.push_or_grow(value),
            RuntimeValue::ValueRef(value) => self.stack_32.push_or_grow(value.raw()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::StackConfig;

    #[test]
    fn locals_are_zeroed_on_reentry() {
        let mut stack = ValueStack::new(&Config::new().with_value_stack(StackConfig::dynamic(0, 16)));
        stack.stack_32.push_or_grow(11).unwrap();
        let params = ValueCounts { c32: 1, c64: 0, c128: 0 };
        let locals = ValueCounts { c32: 3, c64: 1, c128: 1 };
        let operands = ValueCounts { c32: 1, c64: 1, c128: 1 };
        let base = stack.enter_locals(&params, &locals, &operands).unwrap();
        stack.stack_32.set(1, 22);
        stack.stack_64.set(0, 33);
        stack.stack_128.set(0, Value128([44; 16]));
        stack.stack_32.push(55);
        stack.stack_64.push(66);
        stack.stack_128.push(Value128([77; 16]));
        stack.truncate_to_base(base);
        stack.stack_32.push(88);
        stack.enter_locals(&params, &locals, &operands).unwrap();
        assert_eq!(stack.stack_32.as_slice(), [88, 0, 0]);
        assert_eq!(stack.stack_64.as_slice(), [0]);
        assert_eq!(stack.stack_128.as_slice(), [Value128::default()]);
    }

    #[test]
    fn failed_entry_preserves_every_lane() {
        for tail in [false, true] {
            let config =
                Config::new().with_value_stack(StackConfig::dynamic(2, 16)).with_value_stack_128(StackConfig::fixed(2));
            let mut stack = ValueStack::new(&config);
            for value in [11, 22] {
                stack.stack_32.push_or_grow(value).unwrap();
                stack.stack_64.push_or_grow(value as u64).unwrap();
                stack.stack_128.push_or_grow(Value128([value as u8; 16])).unwrap();
            }
            let params = ValueCounts { c32: 1, c64: 1, c128: 1 };
            let locals = ValueCounts { c32: 3, c64: 3, c128: 3 };
            let result = if tail {
                stack.replace_locals(StackBase::default(), &params, &locals, &params)
            } else {
                stack.enter_locals(&params, &locals, &params)
            };
            assert!(matches!(result, Err(Trap::ValueStackOverflow)));
            assert_eq!(stack.stack_32.as_slice(), [11, 22]);
            assert_eq!(stack.stack_64.as_slice(), [11, 22]);
            assert_eq!(stack.stack_128.as_slice(), [Value128([11; 16]), Value128([22; 16])]);
        }
    }

    #[test]
    fn tail_entry_moves_arguments_before_zeroing_locals() {
        let mut stack = ValueStack::new(&Config::new().with_value_stack(StackConfig::dynamic(0, 16)));
        for value in [11, 22, 33] {
            stack.stack_32.push_or_grow(value).unwrap();
            stack.stack_64.push_or_grow(value as u64).unwrap();
            stack.stack_128.push_or_grow(Value128([value as u8; 16])).unwrap();
        }
        let base = StackBase { s32: 1, s64: 1, s128: 1 };
        let params = ValueCounts { c32: 1, c64: 1, c128: 1 };
        let locals = ValueCounts { c32: 3, c64: 3, c128: 3 };
        stack.replace_locals(base, &params, &locals, &params).unwrap();
        assert_eq!(stack.stack_32.as_slice(), [11, 33, 0, 0]);
        assert_eq!(stack.stack_64.as_slice(), [11, 33, 0, 0]);
        assert_eq!(
            stack.stack_128.as_slice(),
            [Value128([11; 16]), Value128([33; 16]), Value128::default(), Value128::default()]
        );
    }
}
