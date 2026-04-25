use crate::{Result, interpreter::simd::Value128};

use super::stack::{CallFrame, ValueStack};
use tinywasm_types::LocalAddr;
use tinywasm_types::{ExternRef, FuncRef, WasmType, WasmValue};

pub(crate) type Value32 = u32;
pub(crate) type Value64 = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValueRef(u32);

impl Default for ValueRef {
    fn default() -> Self {
        Self::NULL
    }
}

impl ValueRef {
    pub(crate) const NULL: Self = Self(u32::MAX);

    #[inline]
    pub(crate) const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[inline]
    pub(crate) const fn from_addr(addr: Option<u32>) -> Self {
        match addr {
            Some(addr) => Self(addr),
            None => Self::NULL,
        }
    }

    #[inline]
    pub(crate) const fn addr(self) -> Option<u32> {
        if self.is_null() { None } else { Some(self.0) }
    }

    #[inline]
    pub(crate) const fn is_null(self) -> bool {
        self.0 == Self::NULL.0
    }

    #[inline]
    pub(crate) const fn raw(self) -> u32 {
        self.0
    }
}

#[allow(private_interfaces)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A untyped WebAssembly value
pub enum TinyWasmValue {
    /// A 32-bit value
    Value32(Value32),
    /// A 64-bit value
    Value64(Value64),
    /// A 128-bit value
    Value128(Value128),
    /// A reference value
    ValueRef(ValueRef),
}

impl TinyWasmValue {
    /// Converts the value to a 32-bit value (returns None if the value is not a 32-bit value)
    pub fn as_32(self) -> Option<Value32> {
        match self {
            Self::Value32(v) => Some(v),
            _ => None,
        }
    }

    /// Converts the value to a 64-bit value (returns None if the value is not a 64-bit value)
    pub fn as_64(self) -> Option<Value64> {
        match self {
            Self::Value64(v) => Some(v),
            _ => None,
        }
    }

    /// Converts the value to a 128-bit value (returns None if the value is not a 128-bit value)
    pub fn as_128(self) -> Option<Value128> {
        match self {
            Self::Value128(v) => Some(v),
            _ => None,
        }
    }

    /// Converts the value to a reference value (returns None if the value is not a reference value)
    #[allow(private_interfaces, dead_code)]
    pub fn as_ref(self) -> Option<ValueRef> {
        match self {
            Self::ValueRef(v) => Some(v),
            _ => None,
        }
    }

    /// Attaches a type to the value (panics if the size of the value is not the same as the type)
    pub fn attach_type(self, ty: WasmType) -> Option<WasmValue> {
        match (self, ty) {
            (Self::Value32(v), WasmType::I32) => Some(WasmValue::I32(v as i32)),
            (Self::Value64(v), WasmType::I64) => Some(WasmValue::I64(v as i64)),
            (Self::Value32(v), WasmType::F32) => Some(WasmValue::F32(f32::from_bits(v))),
            (Self::Value64(v), WasmType::F64) => Some(WasmValue::F64(f64::from_bits(v))),
            (Self::ValueRef(v), WasmType::RefExtern) => Some(WasmValue::RefExtern(ExternRef::from_raw(v.raw()))),
            (Self::ValueRef(v), WasmType::RefFunc) => Some(WasmValue::RefFunc(FuncRef::from_raw(v.raw()))),
            (Self::Value128(v), WasmType::V128) => Some(WasmValue::V128((v).into())),
            (_, WasmType::I32 | WasmType::F32) => None,
            (_, WasmType::I64 | WasmType::F64) => None,
            (_, WasmType::RefExtern | WasmType::RefFunc) => None,
            (_, WasmType::V128) => None,
        }
    }
}

impl From<&WasmValue> for TinyWasmValue {
    fn from(value: &WasmValue) -> Self {
        match value {
            WasmValue::I32(v) => Self::Value32(*v as u32),
            WasmValue::I64(v) => Self::Value64(*v as u64),
            WasmValue::F32(v) => Self::Value32(v.to_bits()),
            WasmValue::F64(v) => Self::Value64(v.to_bits()),
            WasmValue::RefExtern(v) => Self::ValueRef(ValueRef::from_addr(v.addr())),
            WasmValue::RefFunc(v) => Self::ValueRef(ValueRef::from_addr(v.addr())),
            WasmValue::V128(v) => Self::Value128((*v).into()),
        }
    }
}

impl From<WasmValue> for TinyWasmValue {
    fn from(value: WasmValue) -> Self {
        Self::from(&value)
    }
}

impl From<i128> for TinyWasmValue {
    fn from(value: i128) -> Self {
        Self::Value128(Value128::from(value))
    }
}

mod sealed {
    #[expect(unreachable_pub)]
    pub trait Sealed {}
}

pub(crate) trait InternalValue: sealed::Sealed + Into<TinyWasmValue> + Copy + Default {
    fn stack_push(stack: &mut ValueStack, value: Self) -> Result<(), crate::Trap>;
    fn stack_pop(stack: &mut ValueStack) -> Self;
    fn stack_peek(stack: &ValueStack) -> Self;
    fn stack_select(stack: &mut ValueStack) -> Result<(), crate::Trap>;
    fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self;
    fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self);
    fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr);
}

macro_rules! impl_internalvalue {
    (
        $(
            $variant:ident, $stack:ident, $stack_base:ident, $outer:ty,
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
                    stack.$stack.push($to_stack)
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
                fn local_copy(stack: &mut ValueStack, frame: &CallFrame, from: LocalAddr, to: LocalAddr) {
                    let val = stack.$stack.get(frame.locals_base.$stack_base as usize + from as usize);
                    stack.$stack.set(frame.locals_base.$stack_base as usize + to as usize, *val);
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
    Value32,  stack_32,  s32,  u32,      |v| v,               |v| v,               |v| v
    Value64,  stack_64,  s64,  u64,      |v| v,               |v| v,               |v| v
    Value32,  stack_32,  s32,  i32,      |v| v as u32,        |v| v as u32,        |v| v as i32
    Value64,  stack_64,  s64,  i64,      |v| v as u64,        |v| v as u64,        |v| v as i64
    Value32,  stack_32,  s32,  f32,      |v| f32::to_bits(v), |v| f32::to_bits(v), |v| f32::from_bits(v)
    Value64,  stack_64,  s64,  f64,      |v| f64::to_bits(v), |v| f64::to_bits(v), |v| f64::from_bits(v)
    ValueRef, stack_32,  s32,  ValueRef, |v| v,               |v| v.raw(),         |v| ValueRef(v)
    Value128, stack_128, s128, Value128, |v| v,               |v| v,               |v| v
}
