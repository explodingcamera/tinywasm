use super::stack::{CallFrame, ValueStack};
use crate::store::Globals;
use crate::{Result, interpreter::simd::Value128};
use tinywasm_types::{GlobalAddr, LocalAddr, RefValue, WasmValue};

pub(crate) type Value32 = u32;
pub(crate) type Value64 = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Packed internal representation of a WebAssembly reference.
///
/// Unlike the public [`RefValue`], this stores no explicit reference category.
/// Converting it back therefore requires the value's canonical reference type.
pub(crate) struct ValueRef(u32);

impl Default for ValueRef {
    fn default() -> Self {
        Self::NULL
    }
}

impl ValueRef {
    const HOST_ANY_TAG: u32 = 1 << 30;
    pub(crate) const NULL: Self = Self(0);

    #[inline]
    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    pub(crate) const fn from_category_addr(addr: u32) -> Self {
        let Some(raw) = addr.checked_add(1) else { panic!("reference address does not fit in the runtime encoding") };
        let Some(raw) = raw.checked_mul(2) else { panic!("reference address does not fit in the runtime encoding") };
        Self(raw)
    }

    #[inline]
    pub(crate) const fn from_i31(value: i32) -> Self {
        Self(((value as u32) << 1) | 1)
    }

    pub(crate) const fn is_host_any(self) -> bool {
        matches!(self.addr(), Some(addr) if addr & Self::HOST_ANY_TAG != 0)
    }

    #[inline]
    pub(crate) const fn addr(self) -> Option<u32> {
        if self.is_null() || self.is_i31() { None } else { Some(self.0 / 2 - 1) }
    }

    #[inline]
    pub(crate) const fn is_null(self) -> bool {
        self.0 == Self::NULL.0
    }

    #[inline]
    pub(crate) const fn is_i31(self) -> bool {
        self.0 & 1 != 0
    }

    #[inline]
    pub(crate) const fn i31_s(self) -> Option<i32> {
        if self.is_i31() { Some((self.0 as i32) >> 1) } else { None }
    }

    #[inline]
    pub(crate) const fn i31_u(self) -> Option<u32> {
        if self.is_i31() { Some(self.0 >> 1) } else { None }
    }

    #[inline]
    pub(crate) const fn raw(self) -> u32 {
        self.0
    }
}

impl From<RefValue> for ValueRef {
    fn from(value: RefValue) -> Self {
        match value {
            RefValue::Null => Self::NULL,
            RefValue::Func(value) => Self::from_category_addr(value.addr()),
            RefValue::Extern(value) => Self::from_raw(value.raw()),
            RefValue::Exn(value) => Self::from_category_addr(value.addr()),
            RefValue::Any(value) => Self::from_raw(value.raw()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// An untyped internal WebAssembly value.
pub(crate) enum TinyWasmValue {
    /// A 32-bit value.
    Value32(Value32),
    /// A 64-bit value.
    Value64(Value64),
    /// A 128-bit value.
    Value128(Value128),
    /// A reference value.
    ValueRef(ValueRef),
}

impl From<&WasmValue> for TinyWasmValue {
    fn from(value: &WasmValue) -> Self {
        match value {
            WasmValue::I32(v) => Self::Value32(*v as u32),
            WasmValue::I64(v) => Self::Value64(*v as u64),
            WasmValue::F32(v) => Self::Value32(v.to_bits()),
            WasmValue::F64(v) => Self::Value64(v.to_bits()),
            WasmValue::Ref(value) => Self::ValueRef((*value).into()),
            WasmValue::V128(v) => Self::Value128((*v).into()),
        }
    }
}

impl From<WasmValue> for TinyWasmValue {
    fn from(value: WasmValue) -> Self {
        Self::from(&value)
    }
}

impl From<[u8; 16]> for TinyWasmValue {
    fn from(value: [u8; 16]) -> Self {
        Self::Value128(Value128::from(value))
    }
}

mod sealed {
    #[expect(unreachable_pub)]
    pub trait Sealed {}
}

/// Typed access to values in their physical [`ValueStack`] and [`Globals`] lanes.
pub(crate) trait InternalValue: sealed::Sealed + Copy + Default {
    fn stack_push(stack: &mut ValueStack, value: Self) -> Result<(), crate::Trap>;
    fn stack_pop(stack: &mut ValueStack) -> Self;
    fn stack_peek(stack: &ValueStack) -> Self;
    fn stack_select(stack: &mut ValueStack) -> Result<(), crate::Trap>;
    fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self;
    fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self);
    fn local_update(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, f: impl FnOnce(Self) -> Self);
    fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr);
    fn global_get(globals: &Globals, addr: GlobalAddr) -> Self;
    fn global_set(globals: &mut Globals, addr: GlobalAddr, value: Self);
}

macro_rules! impl_internalvalue {
    (
        $(
            $variant:ident, $stack:ident, $stack_base:ident, $global_get:ident, $global_set:ident, $outer:ty,
            |$to_value_v:ident| $to_value:expr,
            |$to_stack_v:ident| $to_stack:expr,
            |$from_stack_v:ident| $from_stack:expr
        )*
    ) => {
        $(
            impl sealed::Sealed for $outer {}

            impl From<$outer> for TinyWasmValue {
                #[inline(always)]
                fn from(value: $outer) -> Self {
                    let $to_value_v = value;
                    TinyWasmValue::$variant($to_value)
                }
            }

            impl InternalValue for $outer {
                #[inline(always)]
                fn stack_push(stack: &mut ValueStack, value: Self) -> Result<(), crate::Trap> {
                    let $to_stack_v = value;
                    if let Err(e) = stack.$stack.push($to_stack) {
                        core::hint::cold_path();
                        return Err(e);
                    }
                    Ok(())
                }

                #[inline(always)]
                fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self {
                    let $from_stack_v = *stack.$stack.get(frame.locals_base.$stack_base as usize + index as usize);
                    $from_stack
                }

                #[inline(always)]
                fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self) {
                    let $to_stack_v = value;
                    let abs_index = frame.locals_base.$stack_base as usize + index as usize;
                    stack.$stack.set(abs_index, $to_stack);
                }

                #[inline(always)]
                fn local_update(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, f: impl FnOnce(Self) -> Self) {
                    let abs_index = frame.locals_base.$stack_base as usize + index as usize;
                    let $from_stack_v = *stack.$stack.get(abs_index);
                    let $to_stack_v = f($from_stack);
                    stack.$stack.set(abs_index, $to_stack);
                }

                #[inline(always)]
                fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr) {
                    let base = frame.locals_base.$stack_base as usize;
                    stack.$stack.copy(base + from as usize, base + to as usize);
                }

                #[inline(always)]
                fn global_get(globals: &Globals, addr: GlobalAddr) -> Self {
                    let $from_stack_v = globals.$global_get(addr);
                    $from_stack
                }

                #[inline(always)]
                fn global_set(globals: &mut Globals, addr: GlobalAddr, value: Self) {
                    let $to_stack_v = value;
                    globals.$global_set(addr, $to_stack);
                }

                #[inline(always)]
                fn stack_pop(stack: &mut ValueStack) -> Self {
                    let $from_stack_v = stack.$stack.pop();
                    $from_stack
                }

                #[inline(always)]
                fn stack_peek(stack: &ValueStack) -> Self {
                    let $from_stack_v = *stack.$stack.last();
                    $from_stack
                }

                #[inline(always)]
                fn stack_select(stack: &mut ValueStack) -> Result<(), crate::Trap> {
                    let cond = stack.stack_32.pop() as i32;
                    let val2 = stack.$stack.pop();

                    if cond == 0 {
                        Self::stack_pop(stack);
                        stack.$stack.push(val2)?;
                    }

                    Ok(())
                }
            }
        )*
    };
}

impl_internalvalue! {
    Value32,  stack_32,  s32,  get_32,  set_32,  u32,      |v| v,               |v| v,               |v| v
    Value64,  stack_64,  s64,  get_64,  set_64,  u64,      |v| v,               |v| v,               |v| v
    Value32,  stack_32,  s32,  get_32,  set_32,  i32,      |v| v as u32,        |v| v as u32,        |v| v as i32
    Value64,  stack_64,  s64,  get_64,  set_64,  i64,      |v| v as u64,        |v| v as u64,        |v| v as i64
    Value32,  stack_32,  s32,  get_32,  set_32,  f32,      |v| f32::to_bits(v), |v| f32::to_bits(v), |v| f32::from_bits(v)
    Value64,  stack_64,  s64,  get_64,  set_64,  f64,      |v| f64::to_bits(v), |v| f64::to_bits(v), |v| f64::from_bits(v)
    ValueRef, stack_32,  s32,  get_32,  set_32,  ValueRef, |v| v,               |v| v.raw(),         |v| ValueRef(v)
    Value128, stack_128, s128, get_128, set_128, Value128, |v| v,               |v| v,               |v| v
}
