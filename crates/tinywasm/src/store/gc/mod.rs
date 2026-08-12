mod arena;
mod object;

use alloc::vec::Vec;
use tinywasm_types::{StorageType, WasmType};

use crate::Trap;
use crate::interpreter::stack::ValueStack;
use crate::interpreter::{InternalValue, TinyWasmValue, Value128, ValueRef};

pub(crate) use arena::{AllocError, Arena, Handle, Trace};
pub(crate) use object::{GcHeap, GcObject};

/// Returns the zero value for a GC field or array element.
pub(crate) fn default_value(storage: StorageType) -> TinyWasmValue {
    match storage {
        StorageType::I8 | StorageType::I16 | StorageType::Value(WasmType::I32 | WasmType::F32) => {
            TinyWasmValue::Value32(0)
        }
        StorageType::Value(WasmType::I64 | WasmType::F64) => TinyWasmValue::Value64(0),
        StorageType::Value(WasmType::V128) => TinyWasmValue::Value128(Value128::from([0; 16])),
        StorageType::Value(WasmType::Ref(_)) => TinyWasmValue::ValueRef(ValueRef::NULL),
    }
}

/// Pops and packs a GC field or array element from the operand stack.
pub(crate) fn pop_value(stack: &mut ValueStack, storage: StorageType) -> TinyWasmValue {
    match storage {
        StorageType::I8 => TinyWasmValue::Value32(i32::stack_pop(stack) as u8 as u32),
        StorageType::I16 => TinyWasmValue::Value32(i32::stack_pop(stack) as u16 as u32),
        StorageType::Value(WasmType::I32 | WasmType::F32) => TinyWasmValue::Value32(u32::stack_pop(stack)),
        StorageType::Value(WasmType::I64 | WasmType::F64) => TinyWasmValue::Value64(u64::stack_pop(stack)),
        StorageType::Value(WasmType::V128) => TinyWasmValue::Value128(Value128::stack_pop(stack)),
        StorageType::Value(WasmType::Ref(_)) => TinyWasmValue::ValueRef(ValueRef::stack_pop(stack)),
    }
}

/// Extends a packed value and pushes it onto the operand stack.
pub(crate) fn push_value(
    stack: &mut ValueStack,
    value: TinyWasmValue,
    storage: StorageType,
    signed: Option<bool>,
) -> Result<(), Trap> {
    let value = match (value, storage, signed) {
        (TinyWasmValue::Value32(value), StorageType::I8, Some(true)) => {
            TinyWasmValue::Value32(value as i8 as i32 as u32)
        }
        (TinyWasmValue::Value32(value), StorageType::I16, Some(true)) => {
            TinyWasmValue::Value32(value as i16 as i32 as u32)
        }
        (TinyWasmValue::Value32(value), StorageType::I8, Some(false)) => TinyWasmValue::Value32(value & u8::MAX as u32),
        (TinyWasmValue::Value32(value), StorageType::I16, Some(false)) => {
            TinyWasmValue::Value32(value & u16::MAX as u32)
        }
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
) -> Result<Vec<TinyWasmValue>, Trap> {
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
        1 => TinyWasmValue::Value32(u32::from(bytes[0])),
        2 => TinyWasmValue::Value32(u32::from(u16::from_le_bytes(bytes.try_into().unwrap()))),
        4 => TinyWasmValue::Value32(u32::from_le_bytes(bytes.try_into().unwrap())),
        8 => TinyWasmValue::Value64(u64::from_le_bytes(bytes.try_into().unwrap())),
        16 => TinyWasmValue::Value128(Value128::from(<[u8; 16]>::try_from(bytes).unwrap())),
        _ => unreachable!(),
    }));
    Ok(values)
}
