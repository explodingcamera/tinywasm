#![allow(missing_docs)]
use super::{call_stack::Locals, ValueStack};
use crate::{Error, Result};
use tinywasm_types::{LocalAddr, ValType, WasmValue};

pub(crate) type Value32 = u32;
pub(crate) type Value64 = u64;
pub(crate) type Value128 = u128;
pub(crate) type ValueRef = Option<u32>;

#[derive(Debug, Clone, Copy, PartialEq)]
/// A untyped WebAssembly value
pub enum TinyWasmValue {
    Value32(Value32),
    Value64(Value64),
    Value128(Value128),
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

impl TinyWasmValue {
    pub fn unwrap_32(&self) -> Value32 {
        match self {
            TinyWasmValue::Value32(v) => *v,
            _ => unreachable!("Expected Value32"),
        }
    }

    pub fn unwrap_64(&self) -> Value64 {
        match self {
            TinyWasmValue::Value64(v) => *v,
            _ => unreachable!("Expected Value64"),
        }
    }

    pub fn unwrap_128(&self) -> Value128 {
        match self {
            TinyWasmValue::Value128(v) => *v,
            _ => unreachable!("Expected Value128"),
        }
    }

    pub fn unwrap_ref(&self) -> ValueRef {
        match self {
            TinyWasmValue::ValueRef(v) => *v,
            _ => unreachable!("Expected ValueRef"),
        }
    }

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
            WasmValue::RefFunc(v) => TinyWasmValue::ValueRef(Some(*v)),
            WasmValue::RefExtern(v) => TinyWasmValue::ValueRef(Some(*v)),
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

pub(crate) trait InternalValue: sealed::Sealed {
    fn stack_push(stack: &mut ValueStack, value: Self);
    fn stack_pop(stack: &mut ValueStack) -> Result<Self>
    where
        Self: Sized;
    fn stack_peek(stack: &ValueStack) -> Result<Self>
    where
        Self: Sized;
    fn local_get(locals: &Locals, index: LocalAddr) -> Result<Self>
    where
        Self: Sized;
    fn local_set(locals: &mut Locals, index: LocalAddr, value: Self) -> Result<()>;
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
                #[inline]
                fn stack_push(stack: &mut ValueStack, value: Self) {
                    stack.$stack.push($to_internal(value));
                }
                #[inline]
                fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
                    stack.$stack.pop().ok_or(Error::ValueStackUnderflow).map($to_outer)
                }
                #[inline]
                fn stack_peek(stack: &ValueStack) -> Result<Self> {
                    stack.$stack.last().copied().ok_or(Error::ValueStackUnderflow).map($to_outer)
                }
                #[inline]
                fn local_get(locals: &Locals, index: LocalAddr) -> Result<Self> {
                    Ok($to_outer(locals.$locals[index as usize]))
                }
                #[inline]
                fn local_set(locals: &mut Locals, index: LocalAddr, value: Self) -> Result<()> {
                    locals.$locals[index as usize] = $to_internal(value);
                    Ok(())
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
