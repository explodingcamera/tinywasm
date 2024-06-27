#![allow(missing_docs)]
use tinywasm_types::{ValType, WasmValue};

use crate::{Error, Result};

use super::{call_stack::Locals, ValueStack};

pub type Value32 = u32;
pub type Value64 = u64;
pub type Value128 = u128;
pub type ValueRef = Option<u32>;

pub const VALUE32: u8 = 0;
pub const VALUE64: u8 = 1;
pub const VALUE128: u8 = 2;
pub const VALUEREF: u8 = 3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct StackLocation {
    pub(crate) s32: u32,
    pub(crate) s64: u32,
    pub(crate) s128: u32,
    pub(crate) sref: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StackHeight {
    pub(crate) s32: u32,
    pub(crate) s64: u32,
    pub(crate) s128: u32,
    pub(crate) sref: u32,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TinyWasmValue {
    Value32(Value32),
    Value64(Value64),
    Value128(Value128),
    ValueRef(ValueRef),
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

impl Default for TinyWasmValue {
    fn default() -> Self {
        TinyWasmValue::Value32(0)
    }
}

impl From<WasmValue> for TinyWasmValue {
    fn from(value: WasmValue) -> Self {
        match value {
            WasmValue::I32(v) => TinyWasmValue::Value32(v as u32),
            WasmValue::I64(v) => TinyWasmValue::Value64(v as u64),
            WasmValue::V128(v) => TinyWasmValue::Value128(v),
            WasmValue::F32(v) => TinyWasmValue::Value32(v.to_bits()),
            WasmValue::F64(v) => TinyWasmValue::Value64(v.to_bits()),
            WasmValue::RefFunc(v) => TinyWasmValue::ValueRef(Some(v)),
            WasmValue::RefExtern(v) => TinyWasmValue::ValueRef(Some(v)),
            WasmValue::RefNull(_) => TinyWasmValue::ValueRef(None),
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

impl From<f32> for TinyWasmValue {
    fn from(value: f32) -> Self {
        TinyWasmValue::Value32(value.to_bits())
    }
}

impl From<f64> for TinyWasmValue {
    fn from(value: f64) -> Self {
        TinyWasmValue::Value64(value.to_bits())
    }
}

impl From<i32> for TinyWasmValue {
    fn from(value: i32) -> Self {
        TinyWasmValue::Value32(value as u32)
    }
}

impl From<i64> for TinyWasmValue {
    fn from(value: i64) -> Self {
        TinyWasmValue::Value64(value as u64)
    }
}

mod sealed {
    #[allow(unreachable_pub)]
    pub trait Sealed {}
}

impl sealed::Sealed for i32 {}
impl sealed::Sealed for f32 {}
impl sealed::Sealed for i64 {}
impl sealed::Sealed for u64 {}
impl sealed::Sealed for f64 {}
impl sealed::Sealed for u32 {}
impl sealed::Sealed for Value128 {}
impl sealed::Sealed for ValueRef {}

// TODO: this can be made a bit more maintainable by using a macro

pub(crate) trait InternalValue: sealed::Sealed {
    fn stack_push(stack: &mut ValueStack, value: Self);
    fn stack_pop(stack: &mut ValueStack) -> Result<Self>
    where
        Self: Sized;
    fn stack_peek(stack: &ValueStack) -> Result<Self>
    where
        Self: Sized;

    fn local_get(locals: &Locals, index: u32) -> Result<Self>
    where
        Self: Sized;

    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()>;
}

impl InternalValue for i32 {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_32.push(value as u32);
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_32.pop().ok_or(Error::ValueStackUnderflow).map(|v| v as i32)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_32.last().ok_or(Error::ValueStackUnderflow).map(|v| *v as i32)
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(locals.locals_32[index as usize] as i32)
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_32[index as usize] = value as u32;
        Ok(())
    }
}

impl InternalValue for f32 {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_32.push(value.to_bits());
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_32.pop().ok_or(Error::ValueStackUnderflow).map(f32::from_bits)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_32.last().ok_or(Error::ValueStackUnderflow).map(|v| f32::from_bits(*v))
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(f32::from_bits(locals.locals_32[index as usize]))
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_32[index as usize] = value.to_bits();
        Ok(())
    }
}

impl InternalValue for i64 {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_64.push(value as u64);
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_64.pop().ok_or(Error::ValueStackUnderflow).map(|v| v as i64)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_64.last().ok_or(Error::ValueStackUnderflow).map(|v| *v as i64)
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(locals.locals_64[index as usize] as i64)
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_64[index as usize] = value as u64;
        Ok(())
    }
}

impl InternalValue for u64 {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_64.push(value);
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_64.pop().ok_or(Error::ValueStackUnderflow)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_64.last().ok_or(Error::ValueStackUnderflow).copied()
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(locals.locals_64[index as usize])
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_64[index as usize] = value;
        Ok(())
    }
}

impl InternalValue for f64 {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_64.push(value.to_bits());
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_64.pop().ok_or(Error::ValueStackUnderflow).map(f64::from_bits)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_64.last().ok_or(Error::ValueStackUnderflow).map(|v| f64::from_bits(*v))
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(f64::from_bits(locals.locals_64[index as usize]))
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_64[index as usize] = value.to_bits();
        Ok(())
    }
}

impl InternalValue for u32 {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_32.push(value);
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_32.pop().ok_or(Error::ValueStackUnderflow)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_32.last().ok_or(Error::ValueStackUnderflow).copied()
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(locals.locals_32[index as usize])
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_32[index as usize] = value;
        Ok(())
    }
}

impl InternalValue for Value128 {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_128.push(value);
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_128.pop().ok_or(Error::ValueStackUnderflow)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_128.last().ok_or(Error::ValueStackUnderflow).copied()
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(locals.locals_128[index as usize])
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_128[index as usize] = value;
        Ok(())
    }
}

impl InternalValue for ValueRef {
    #[inline]
    fn stack_push(stack: &mut ValueStack, value: Self) {
        stack.stack_ref.push(value);
    }
    #[inline]
    fn stack_pop(stack: &mut ValueStack) -> Result<Self> {
        stack.stack_ref.pop().ok_or(Error::ValueStackUnderflow)
    }
    #[inline]
    fn stack_peek(stack: &ValueStack) -> Result<Self> {
        stack.stack_ref.last().ok_or(Error::ValueStackUnderflow).copied()
    }
    #[inline]
    fn local_get(locals: &Locals, index: u32) -> Result<Self> {
        Ok(locals.locals_ref[index as usize])
    }
    #[inline]
    fn local_set(locals: &mut Locals, index: u32, value: Self) -> Result<()> {
        locals.locals_ref[index as usize] = value;
        Ok(())
    }
}
