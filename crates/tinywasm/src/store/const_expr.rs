use alloc::{format, vec::Vec};
use core::hint::cold_path;
use tinywasm_types::*;

use super::{State, default_value};
use crate::interpreter::{TinyWasmValue, ValueRef};
use crate::{Error, Result, Trap};

#[inline]
pub(super) fn eval_const(
    state: &mut State,
    instructions: &[ConstInstruction],
    global_addrs: &[GlobalAddr],
    func_addrs: &[FuncAddr],
    type_addrs: &[TypeAddr],
) -> Result<TinyWasmValue> {
    use ConstInstruction::*;

    let global_value = |state: &State, index: u32| -> Result<TinyWasmValue> {
        let addr =
            *global_addrs.get(index as usize).ok_or_else(|| Error::Other(format!("global {index} not found")))?;
        Ok(state.globals.get(addr))
    };
    let func_ref = |index: u32| -> Result<ValueRef> {
        let addr =
            *func_addrs.get(index as usize).ok_or_else(|| Error::Other(format!("function {index} not found")))?;
        Ok(ValueRef::from_category_addr(addr))
    };
    let type_addr = |index: TypeAddr| -> Result<TypeAddr> {
        type_addrs.get(index as usize).copied().ok_or_else(|| Error::other("GC constant type not found"))
    };
    let pop_value = |stack: &mut Vec<TinyWasmValue>, storage: StorageType| -> Result<TinyWasmValue> {
        let value = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
        match (storage, value) {
            (StorageType::I8, TinyWasmValue::Value32(value)) => Ok(TinyWasmValue::Value32(value as u8 as u32)),
            (StorageType::I16, TinyWasmValue::Value32(value)) => Ok(TinyWasmValue::Value32(value as u16 as u32)),
            (StorageType::Value(WasmType::I32 | WasmType::F32), value @ TinyWasmValue::Value32(_))
            | (StorageType::Value(WasmType::I64 | WasmType::F64), value @ TinyWasmValue::Value64(_))
            | (StorageType::Value(WasmType::V128), value @ TinyWasmValue::Value128(_))
            | (StorageType::Value(WasmType::Ref(_)), value @ TinyWasmValue::ValueRef(_)) => Ok(value),
            _ => Err(Error::other("type mismatch in GC constant")),
        }
    };

    if let [instruction] = instructions {
        match instruction {
            I32Const(value) => return Ok(TinyWasmValue::Value32(*value as u32)),
            I64Const(value) => return Ok(TinyWasmValue::Value64(*value as u64)),
            F32Const(value) => return Ok(TinyWasmValue::Value32(value.to_bits())),
            F64Const(value) => return Ok(TinyWasmValue::Value64(value.to_bits())),
            V128Const(value) => return Ok(TinyWasmValue::Value128((*value).into())),
            GlobalGet(index) => return global_value(state, *index),
            Ref(RefValue::Null) => return Ok(TinyWasmValue::ValueRef(ValueRef::NULL)),
            Ref(RefValue::Func(func)) => return Ok(TinyWasmValue::ValueRef(func_ref(func.addr())?)),
            _ => {}
        }
    }

    let mut stack = Vec::new();
    for instruction in instructions {
        match instruction {
            I32Const(value) => stack.push(TinyWasmValue::Value32(*value as u32)),
            I64Const(value) => stack.push(TinyWasmValue::Value64(*value as u64)),
            F32Const(value) => stack.push(TinyWasmValue::Value32(value.to_bits())),
            F64Const(value) => stack.push(TinyWasmValue::Value64(value.to_bits())),
            V128Const(value) => stack.push(TinyWasmValue::Value128((*value).into())),
            GlobalGet(index) => stack.push(global_value(state, *index)?),
            Ref(RefValue::Null) => stack.push(TinyWasmValue::ValueRef(ValueRef::NULL)),
            Ref(RefValue::Func(func)) => stack.push(TinyWasmValue::ValueRef(func_ref(func.addr())?)),
            Ref(_) => {
                cold_path();
                return Err(Error::other("unsupported reference constant"));
            }
            RefI31 => {
                let value = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                let TinyWasmValue::Value32(value) = value else {
                    return Err(Error::other("type mismatch in const ref.i31"));
                };
                stack.push(TinyWasmValue::ValueRef(ValueRef::from_i31(value as i32)));
            }
            AnyConvertExtern | ExternConvertAny => {
                let value = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                if !matches!(value, TinyWasmValue::ValueRef(_)) {
                    return Err(Error::other("type mismatch in const reference conversion"));
                }
                stack.push(value);
            }
            StructNew(type_index) | StructNewDefault(type_index) => {
                let type_addr = type_addr(*type_index)?;
                let fields = state
                    .get_type(type_addr)
                    .as_struct()
                    .ok_or_else(|| Error::other("GC constant type is not a struct"))?
                    .fields
                    .clone();
                let default = matches!(instruction, StructNewDefault(_));
                let mut values = Vec::new();
                values.try_reserve_exact(fields.len()).map_err(|_| Trap::OutOfMemory)?;
                if default {
                    values.extend(fields.iter().map(|field| default_value(field.storage)));
                } else {
                    for field in fields.iter().rev() {
                        values.push(pop_value(&mut stack, field.storage)?);
                    }
                    values.reverse();
                }
                let roots = stack.iter().filter_map(|value| value.as_ref()).map(ValueRef::raw);
                let reference = state.alloc_gc_object(type_addr, values, roots)?;
                stack.push(TinyWasmValue::ValueRef(reference));
            }
            ArrayNew(type_index) | ArrayNewDefault(type_index) => {
                let type_addr = type_addr(*type_index)?;
                let storage = state
                    .get_type(type_addr)
                    .as_array()
                    .ok_or_else(|| Error::other("GC constant type is not an array"))?
                    .field
                    .storage;
                let Some(TinyWasmValue::Value32(len)) = stack.pop() else {
                    return Err(Error::other("type mismatch in const array length"));
                };
                let value = if matches!(instruction, ArrayNewDefault(_)) {
                    default_value(storage)
                } else {
                    pop_value(&mut stack, storage)?
                };
                let mut values = Vec::new();
                values.try_reserve_exact(len as usize).map_err(|_| Trap::OutOfMemory)?;
                values.resize(len as usize, value);
                let roots = stack.iter().filter_map(|value| value.as_ref()).map(ValueRef::raw);
                let reference = state.alloc_gc_object(type_addr, values, roots)?;
                stack.push(TinyWasmValue::ValueRef(reference));
            }
            ArrayNewFixed(type_index, len) => {
                let type_addr = type_addr(*type_index)?;
                let storage = state
                    .get_type(type_addr)
                    .as_array()
                    .ok_or_else(|| Error::other("GC constant type is not an array"))?
                    .field
                    .storage;
                let mut values = Vec::new();
                values.try_reserve_exact(*len as usize).map_err(|_| Trap::OutOfMemory)?;
                for _ in 0..*len {
                    values.push(pop_value(&mut stack, storage)?);
                }
                values.reverse();
                let roots = stack.iter().filter_map(|value| value.as_ref()).map(ValueRef::raw);
                let reference = state.alloc_gc_object(type_addr, values, roots)?;
                stack.push(TinyWasmValue::ValueRef(reference));
            }
            I32Add | I32Sub | I32Mul => {
                let rhs = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                let lhs = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                let (TinyWasmValue::Value32(lhs), TinyWasmValue::Value32(rhs)) = (lhs, rhs) else {
                    cold_path();
                    return Err(Error::other("type mismatch in const i32 op"));
                };
                let out = match instruction {
                    I32Add => (lhs as i32).wrapping_add(rhs as i32),
                    I32Sub => (lhs as i32).wrapping_sub(rhs as i32),
                    I32Mul => (lhs as i32).wrapping_mul(rhs as i32),
                    _ => unreachable!(),
                };
                stack.push(TinyWasmValue::Value32(out as u32));
            }
            I64Add | I64Sub | I64Mul => {
                let rhs = stack.pop();
                let lhs = stack.pop();
                let (Some(TinyWasmValue::Value64(lhs)), Some(TinyWasmValue::Value64(rhs))) = (lhs, rhs) else {
                    cold_path();
                    return Err(Error::other("type mismatch in const i64 op"));
                };
                let out = match instruction {
                    I64Add => (lhs as i64).wrapping_add(rhs as i64),
                    I64Sub => (lhs as i64).wrapping_sub(rhs as i64),
                    I64Mul => (lhs as i64).wrapping_mul(rhs as i64),
                    _ => unreachable!(),
                };
                stack.push(TinyWasmValue::Value64(out as u64));
            }
        }
    }

    let Some(value) = stack.pop() else {
        cold_path();
        return Err(Error::other("empty const expression"));
    };
    if !stack.is_empty() {
        cold_path();
        return Err(Error::other("const expression did not reduce to single value"));
    }
    Ok(value)
}
