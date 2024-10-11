use crate::Result;
use tinywasm_types::{LocalAddr, ValType, WasmValue};

use super::stack::{Locals, ValueStack};

pub(crate) type Value32 = u32;
pub(crate) type Value64 = u64;
pub(crate) type Value128 = u128;
pub(crate) type ValueRef = Option<u32>;

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, Clone, Copy)]
pub(crate) struct StackLocation {
    pub(crate) s32: u32,
    pub(crate) s64: u32,
    pub(crate) s128: u32,
    pub(crate) sref: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StackHeight {
    pub(crate) s32: u16,
    pub(crate) s64: u16,
    pub(crate) s128: u16,
    pub(crate) sref: u16,
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
        for val_type in value {
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

impl TinyWasmValue {
    /// Asserts that the value is a 32-bit value and returns it (panics if the value is the wrong size)
    pub fn unwrap_32(&self) -> Value32 {
        match self {
            TinyWasmValue::Value32(v) => *v,
            _ => unreachable!("Expected Value32"),
        }
    }

    /// Asserts that the value is a 64-bit value and returns it (panics if the value is the wrong size)
    pub fn unwrap_64(&self) -> Value64 {
        match self {
            TinyWasmValue::Value64(v) => *v,
            _ => unreachable!("Expected Value64"),
        }
    }

    /// Asserts that the value is a 128-bit value and returns it (panics if the value is the wrong size)
    pub fn unwrap_128(&self) -> Value128 {
        match self {
            TinyWasmValue::Value128(v) => *v,
            _ => unreachable!("Expected Value128"),
        }
    }

    /// Asserts that the value is a reference value and returns it (panics if the value is the wrong size)
    pub fn unwrap_ref(&self) -> ValueRef {
        match self {
            TinyWasmValue::ValueRef(v) => *v,
            _ => unreachable!("Expected ValueRef"),
        }
    }

    /// Attaches a type to the value (panics if the size of the value is not the same as the type)
    pub fn attach_type(&self, ty: ValType) -> WasmValue {
        match ty {
            ValType::I32 => WasmValue::I32(self.unwrap_32() as i32),
            ValType::I64 => WasmValue::I64(self.unwrap_64() as i64),
            ValType::F32 => WasmValue::F32(f32::from_bits(self.unwrap_32())),
            ValType::F64 => WasmValue::F64(f64::from_bits(self.unwrap_64())),
            ValType::V128 => WasmValue::V128(self.unwrap_128()),
            ValType::RefExtern => match self.unwrap_ref() {
                Some(v) => WasmValue::RefExtern(v),
                None => WasmValue::RefNull(ValType::RefExtern),
            },
            ValType::RefFunc => match self.unwrap_ref() {
                Some(v) => WasmValue::RefFunc(v),
                None => WasmValue::RefNull(ValType::RefFunc),
            },
        }
    }
}

impl From<&WasmValue> for TinyWasmValue {
    fn from(value: &WasmValue) -> Self {
        match value {
            WasmValue::I32(v) => TinyWasmValue::Value32(*v as u32),
            WasmValue::I64(v) => TinyWasmValue::Value64(*v as u64),
            WasmValue::V128(v) => TinyWasmValue::Value128(*v),
            WasmValue::F32(v) => TinyWasmValue::Value32(v.to_bits()),
            WasmValue::F64(v) => TinyWasmValue::Value64(v.to_bits()),
            WasmValue::RefFunc(v) | WasmValue::RefExtern(v) => TinyWasmValue::ValueRef(Some(*v)),
            WasmValue::RefNull(_) => TinyWasmValue::ValueRef(None),
        }
    }
}

impl From<WasmValue> for TinyWasmValue {
    fn from(value: WasmValue) -> Self {
        TinyWasmValue::from(&value)
    }
}

mod sealed {
    #[allow(unreachable_pub)]
    pub trait Sealed {}
}

pub(crate) trait InternalValue: sealed::Sealed + Into<TinyWasmValue> {
    fn stack_push(stack: &mut ValueStack, value: Self);
    fn replace_top(stack: &mut ValueStack, func: fn(Self) -> Result<Self>) -> Result<()>
    where
        Self: Sized;
    fn stack_calculate(stack: &mut ValueStack, func: fn(Self, Self) -> Result<Self>) -> Result<()>
    where
        Self: Sized;

    fn stack_pop(stack: &mut ValueStack) -> Self
    where
        Self: Sized;
    fn stack_peek(stack: &ValueStack) -> Self
    where
        Self: Sized;
    fn local_get(locals: &Locals, index: LocalAddr) -> Self;
    fn local_set(locals: &mut Locals, index: LocalAddr, value: Self);
}

macro_rules! impl_internalvalue {
    ($( $variant:ident, $stack:ident, $locals:ident, $internal:ty, $outer:ty, $to_internal:expr, $to_outer:expr )*) => {
        $(
            impl sealed::Sealed for $outer {}

            impl From<$outer> for TinyWasmValue {
                fn from(value: $outer) -> Self {
                    TinyWasmValue::$variant($to_internal(value))
                }
            }

            impl InternalValue for $outer {
                #[inline(always)]
                fn stack_push(stack: &mut ValueStack, value: Self) {
                    stack.$stack.push($to_internal(value));
                }

                #[inline(always)]
                fn stack_pop(stack: &mut ValueStack) -> Self {
                    match stack.$stack.pop() {
                        Some(v) => $to_outer(v),
                        None => unreachable!("ValueStack underflow, this is a bug"),
                    }
                }

                #[inline(always)]
                fn stack_peek(stack: &ValueStack) -> Self {
                    match stack.$stack.last() {
                        Some(v) => $to_outer(*v),
                        None => unreachable!("ValueStack underflow, this is a bug"),
                    }
                }

                #[inline(always)]
                fn stack_calculate(stack: &mut ValueStack, func: fn(Self, Self) -> Result<Self>) -> Result<()> {
                    let v2 = stack.$stack.pop();
                    let v1 = stack.$stack.last_mut();
                    let (Some(v1), Some(v2)) = (v1, v2) else {
                         unreachable!("ValueStack underflow, this is a bug");
                    };

                    *v1 = $to_internal(func($to_outer(*v1), $to_outer(v2))?);
                    return Ok(())
                }

                #[inline(always)]
                fn replace_top(stack: &mut ValueStack, func: fn(Self) -> Result<Self>) -> Result<()> {
                    let Some(v) = stack.$stack.last_mut() else {
                        unreachable!("ValueStack underflow, this is a bug");
                    };

                    *v = $to_internal(func($to_outer(*v))?);
                    Ok(())
                }

                #[inline(always)]
                fn local_get(locals: &Locals, index: LocalAddr) -> Self {
                    match locals.$locals.get(index as usize) {
                        Some(v) => $to_outer(*v),
                        None => unreachable!("Local variable out of bounds, this is a bug"),
                    }
                }

                #[inline(always)]
                fn local_set(locals: &mut Locals, index: LocalAddr, value: Self) {
                    match locals.$locals.get_mut(index as usize) {
                        Some(v) => *v = $to_internal(value),
                        None => unreachable!("Local variable out of bounds, this is a bug"),
                    }
                }
            }
        )*
    };
}

impl_internalvalue! {
    Value32, stack_32, locals_32, u32, u32, |v| v, |v| v
    Value64, stack_64, locals_64, u64, u64, |v| v, |v| v
    Value32, stack_32, locals_32, u32, i32, |v| v as u32, |v: u32| v as i32
    Value64, stack_64, locals_64, u64, i64, |v| v as u64, |v| v as i64
    Value32, stack_32, locals_32, u32, f32, f32::to_bits, f32::from_bits
    Value64, stack_64, locals_64, u64, f64, f64::to_bits, f64::from_bits
    Value128, stack_128, locals_128, Value128, Value128, |v| v, |v| v
    ValueRef, stack_ref, locals_ref, ValueRef, ValueRef, |v| v, |v| v
}
