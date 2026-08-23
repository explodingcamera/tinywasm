mod arena;
mod object;
mod roots;

use alloc::vec::Vec;
use tinywasm_types::{StorageType, WasmType};

use crate::Trap;
use crate::interpreter::stack::ValueStack;
use crate::interpreter::{InternalValue, RuntimeValue, Value128, ValueRef};

pub(crate) use arena::{AllocError, Arena, Handle, Trace};
pub(crate) use object::{GcHeap, GcObjectKind};
pub(crate) use roots::Roots;

/// Returns the zero value for a GC field or array element.
pub(crate) fn default_value(storage: StorageType) -> RuntimeValue {
    match storage {
        StorageType::I8 | StorageType::I16 | StorageType::Value(WasmType::I32 | WasmType::F32) => {
            RuntimeValue::Value32(0)
        }
        StorageType::Value(WasmType::I64 | WasmType::F64) => RuntimeValue::Value64(0),
        StorageType::Value(WasmType::V128) => RuntimeValue::Value128(Value128([0; 16])),
        StorageType::Value(WasmType::Ref(_)) => RuntimeValue::ValueRef(ValueRef::NULL),
    }
}

/// Pops and packs a GC field or array element from the operand stack.
pub(crate) fn pop_value(stack: &mut ValueStack, storage: StorageType) -> RuntimeValue {
    match storage {
        StorageType::I8 => RuntimeValue::Value32(i32::stack_pop(stack) as u8 as u32),
        StorageType::I16 => RuntimeValue::Value32(i32::stack_pop(stack) as u16 as u32),
        StorageType::Value(WasmType::I32 | WasmType::F32) => RuntimeValue::Value32(u32::stack_pop(stack)),
        StorageType::Value(WasmType::I64 | WasmType::F64) => RuntimeValue::Value64(u64::stack_pop(stack)),
        StorageType::Value(WasmType::V128) => RuntimeValue::Value128(Value128::stack_pop(stack)),
        StorageType::Value(WasmType::Ref(_)) => RuntimeValue::ValueRef(ValueRef::stack_pop(stack)),
    }
}

/// Extends a packed value and pushes it onto the operand stack.
pub(crate) fn push_value(
    stack: &mut ValueStack,
    value: RuntimeValue,
    storage: StorageType,
    signed: Option<bool>,
) -> Result<(), Trap> {
    let value = match (value, storage, signed) {
        (RuntimeValue::Value32(value), StorageType::I8, Some(true)) => RuntimeValue::Value32(value as i8 as i32 as u32),
        (RuntimeValue::Value32(value), StorageType::I16, Some(true)) => {
            RuntimeValue::Value32(value as i16 as i32 as u32)
        }
        (RuntimeValue::Value32(value), StorageType::I8, Some(false)) => RuntimeValue::Value32(value & u8::MAX as u32),
        (RuntimeValue::Value32(value), StorageType::I16, Some(false)) => RuntimeValue::Value32(value & u16::MAX as u32),
        (value, _, None) => value,
        _ => unreachable!("validated packed field access"),
    };
    stack.push_dyn(value)
}

/// Decodes numeric array elements from a data segment.
pub(crate) fn decode_data(
    storage: StorageType,
    data: &[u8],
    src: usize,
    len: usize,
) -> Result<Vec<RuntimeValue>, Trap> {
    let width = match storage {
        StorageType::I8 => 1,
        StorageType::I16 => 2,
        StorageType::Value(WasmType::I32 | WasmType::F32) => 4,
        StorageType::Value(WasmType::I64 | WasmType::F64) => 8,
        StorageType::Value(WasmType::V128) => 16,
        StorageType::Value(WasmType::Ref(_)) => unreachable!("array.new_data reference element"),
    };
    let Some(end) = len.checked_mul(width).and_then(|bytes| src.checked_add(bytes)).filter(|&end| end <= data.len())
    else {
        return Err(Trap::MemoryOutOfBounds { offset: src, len: len.saturating_mul(width), max: data.len() });
    };
    let mut values = Vec::new();
    cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
    values.extend(data[src..end].chunks_exact(width).map(|bytes| match width {
        1 => RuntimeValue::Value32(u32::from(bytes[0])),
        2 => RuntimeValue::Value32(u32::from(u16::from_le_bytes(bytes.try_into().unwrap()))),
        4 => RuntimeValue::Value32(u32::from_le_bytes(bytes.try_into().unwrap())),
        8 => RuntimeValue::Value64(u64::from_le_bytes(bytes.try_into().unwrap())),
        16 => RuntimeValue::Value128(Value128(<[u8; 16]>::try_from(bytes).unwrap())),
        _ => unreachable!(),
    }));
    Ok(values)
}
