use crate::{Result, interpreter::value128::Value128};

use super::stack::{CallFrame, ValueStack};
use tinywasm_types::LocalAddr;
use tinywasm_types::{ExternRef, FuncRef, ValType, WasmValue};

pub(crate) type Value32 = u32;
pub(crate) type Value64 = u64;
pub(crate) type ValueRef = Option<u32>;

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
    /// Asserts that the value is a 32-bit value and returns it (panics if the value is the wrong size)
    pub fn unwrap_32(&self) -> Value32 {
        match self {
            Self::Value32(v) => *v,
            _ => panic!("Expected Value32"),
        }
    }

    /// Asserts that the value is a 64-bit value and returns it (panics if the value is the wrong size)
    pub fn unwrap_64(&self) -> Value64 {
        match self {
            Self::Value64(v) => *v,
            _ => panic!("Expected Value64"),
        }
    }

    /// Asserts that the value is a 128-bit value and returns it (panics if the value is the wrong size)
    pub fn unwrap_128(&self) -> Value128 {
        match self {
            Self::Value128(v) => *v,
            _ => panic!("Expected Value128"),
        }
    }

    /// Asserts that the value is a reference value and returns it (panics if the value is the wrong size)
    pub fn unwrap_ref(&self) -> ValueRef {
        match self {
            Self::ValueRef(v) => *v,
            _ => panic!("Expected ValueRef"),
        }
    }

    /// Attaches a type to the value (panics if the size of the value is not the same as the type)
    pub fn attach_type(&self, ty: ValType) -> WasmValue {
        match (self, ty) {
            (Self::Value32(v), ValType::I32) => WasmValue::I32(*v as i32),
            (Self::Value64(v), ValType::I64) => WasmValue::I64(*v as i64),
            (Self::Value32(v), ValType::F32) => WasmValue::F32(f32::from_bits(*v)),
            (Self::Value64(v), ValType::F64) => WasmValue::F64(f64::from_bits(*v)),
            (Self::ValueRef(v), ValType::RefExtern) => WasmValue::RefExtern(ExternRef::new(*v)),
            (Self::ValueRef(v), ValType::RefFunc) => WasmValue::RefFunc(FuncRef::new(*v)),
            (Self::Value128(v), ValType::V128) => WasmValue::V128((*v).into()),

            (_, ValType::I32 | ValType::F32) => panic!("Expected Value32"),
            (_, ValType::I64 | ValType::F64) => panic!("Expected Value64"),
            (_, ValType::RefExtern | ValType::RefFunc) => panic!("Expected ValueRef"),
            (_, ValType::V128) => panic!("Expected Value128"),
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
            WasmValue::RefExtern(v) => Self::ValueRef(v.addr()),
            WasmValue::RefFunc(v) => Self::ValueRef(v.addr()),
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
    fn stack_push(stack: &mut ValueStack, value: Self) -> Result<()>;
    fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self;
    fn local_update(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, func: impl FnOnce(&mut Self));
    fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self);
    fn stack_pop(stack: &mut ValueStack) -> Self;
    fn stack_peek(stack: &ValueStack) -> Self;
    fn stack_apply1(stack: &mut ValueStack, func: impl FnOnce(Self) -> Result<Self>) -> Result<()>;
    fn stack_apply2(stack: &mut ValueStack, func: impl FnOnce(Self, Self) -> Result<Self>) -> Result<()>;
    fn stack_apply3(stack: &mut ValueStack, func: impl FnOnce(Self, Self, Self) -> Result<Self>) -> Result<()>;
}

macro_rules! impl_internalvalue {
    ($( $variant:ident, $stack:ident, $stack_base:ident, $outer:ty, $to_internal:expr, $to_outer:expr )*) => {
        $(
            impl sealed::Sealed for $outer {}

            impl From<$outer> for TinyWasmValue {
                fn from(value: $outer) -> Self {
                    TinyWasmValue::$variant($to_internal(value))
                }
            }

            impl InternalValue for $outer {
                #[inline(always)]
                fn stack_push(stack: &mut ValueStack, value: Self) -> Result<()> {
                    stack.$stack.push($to_internal(value))
                }

                #[inline(always)]
                fn local_get(stack: &ValueStack, frame: &CallFrame, index: LocalAddr) -> Self {
                    $to_outer(stack.$stack.get(frame.locals_base.$stack_base as usize + index as usize))
                }

                #[inline(always)]
                fn local_update(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, func: impl FnOnce(&mut Self)) {
                    let slot = stack.$stack.get_mut(frame.locals_base.$stack_base as usize + index as usize);
                    let mut value = $to_outer(*slot);
                    func(&mut value);
                    *slot = $to_internal(value);
                }

                #[inline(always)]
                fn local_set(stack: &mut ValueStack, frame: &CallFrame, index: LocalAddr, value: Self) {
                    stack.$stack.set(frame.locals_base.$stack_base as usize + index as usize, $to_internal(value));
                }

                #[inline(always)]
                fn stack_pop(stack: &mut ValueStack) -> Self {
                    $to_outer(stack.$stack.pop())
                }

                #[inline(always)]
                fn stack_peek(stack: &ValueStack) -> Self {
                    $to_outer(*stack.$stack.last())
                }

                #[inline(always)]
                fn stack_apply1(stack: &mut ValueStack, func: impl FnOnce(Self) -> Result<Self>) -> Result<()> {
                    let top = stack.$stack.last_mut();
                    *top = $to_internal(func($to_outer(*top))?);
                    Ok(())
                }

                #[inline(always)]
                fn stack_apply2(stack: &mut ValueStack, func: impl FnOnce(Self, Self) -> Result<Self>) -> Result<()> {
                    let v2 = stack.$stack.pop();
                    let v1 = stack.$stack.last_mut();
                    *v1 = $to_internal(func($to_outer(*v1), $to_outer(v2))?);
                    Ok(())
                }

                #[inline(always)]
                fn stack_apply3(stack: &mut ValueStack, func: impl FnOnce(Self, Self, Self) -> Result<Self>) -> Result<()> {
                    let v3 = stack.$stack.pop();
                    let v2 = stack.$stack.pop();
                    let v1 = stack.$stack.last_mut();
                    *v1 = $to_internal(func($to_outer(*v1), $to_outer(v2), $to_outer(v3))?);
                    Ok(())
                }
            }
        )*
    };
}

impl_internalvalue! {
    Value32, stack_32, s32, u32, |v| v, |v| v
    Value64, stack_64, s64, u64, |v| v, |v| v
    Value32, stack_32, s32, i32, |v: i32| u32::from_ne_bytes(v.to_ne_bytes()), |v: u32| i32::from_ne_bytes(v.to_ne_bytes())
    Value64, stack_64, s64, i64, |v: i64| u64::from_ne_bytes(v.to_ne_bytes()), |v: u64| i64::from_ne_bytes(v.to_ne_bytes())
    Value32, stack_32, s32, f32, f32::to_bits, f32::from_bits
    Value64, stack_64, s64, f64, f64::to_bits, f64::from_bits
    ValueRef, stack_ref, sref, ValueRef, |v| v, |v| v
    Value128, stack_128, s128, Value128, |v| v, |v| v
}
