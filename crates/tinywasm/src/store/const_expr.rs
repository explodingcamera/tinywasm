use alloc::{format, vec::Vec};
use tinywasm_types::*;

use super::{State, default_value};
use crate::interpreter::{TinyWasmValue, ValueRef};
use crate::{Error, Result, Trap};

fn resolve<T: Copy>(items: &[T], index: u32, kind: &str) -> Result<T> {
    items.get(index as usize).copied().ok_or_else(|| Error::Other(format!("{kind} {index} not found")))
}

fn pop_value(stack: &mut Vec<TinyWasmValue>, storage: StorageType) -> Result<TinyWasmValue> {
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
}

fn value_ref(value: &TinyWasmValue) -> Option<ValueRef> {
    match value {
        TinyWasmValue::ValueRef(value) => Some(*value),
        _ => None,
    }
}

fn alloc_object(
    state: &mut State,
    stack: &mut Vec<TinyWasmValue>,
    type_addr: TypeAddr,
    values: Vec<TinyWasmValue>,
) -> Result<()> {
    let roots = stack.iter().filter_map(value_ref).map(ValueRef::raw);
    let reference = state.alloc_gc_object(type_addr, values, roots)?;
    stack.push(TinyWasmValue::ValueRef(reference));
    Ok(())
}

#[inline]
pub(super) fn eval_const(
    state: &mut State,
    instructions: &[ConstInstruction],
    global_addrs: &[GlobalAddr],
    func_addrs: &[FuncAddr],
    type_addrs: &[TypeAddr],
) -> Result<TinyWasmValue> {
    use ConstInstruction::*;

    if let [instruction] = instructions {
        match instruction {
            I32Const(value) => return Ok(TinyWasmValue::Value32(*value as u32)),
            I64Const(value) => return Ok(TinyWasmValue::Value64(*value as u64)),
            F32Const(value) => return Ok(TinyWasmValue::Value32(value.to_bits())),
            F64Const(value) => return Ok(TinyWasmValue::Value64(value.to_bits())),
            V128Const(value) => return Ok(TinyWasmValue::Value128((*value).into())),
            GlobalGet32(index) => {
                return Ok(TinyWasmValue::Value32(state.globals.get_32(resolve(global_addrs, *index, "global")?)));
            }
            GlobalGet64(index) => {
                return Ok(TinyWasmValue::Value64(state.globals.get_64(resolve(global_addrs, *index, "global")?)));
            }
            GlobalGet128(index) => {
                return Ok(TinyWasmValue::Value128(state.globals.get_128(resolve(global_addrs, *index, "global")?)));
            }
            GlobalGetRef(index) => {
                let value = state.globals.get_32(resolve(global_addrs, *index, "global")?);
                return Ok(TinyWasmValue::ValueRef(ValueRef::from_raw(value)));
            }
            Ref(RefValue::Null) => return Ok(TinyWasmValue::ValueRef(ValueRef::NULL)),
            Ref(RefValue::Func(func)) => {
                let addr = resolve(func_addrs, func.addr(), "function")?;
                return Ok(TinyWasmValue::ValueRef(ValueRef::from_category_addr(addr)));
            }
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
            GlobalGet32(index) => {
                stack.push(TinyWasmValue::Value32(state.globals.get_32(resolve(global_addrs, *index, "global")?)));
            }
            GlobalGet64(index) => {
                stack.push(TinyWasmValue::Value64(state.globals.get_64(resolve(global_addrs, *index, "global")?)));
            }
            GlobalGet128(index) => {
                stack.push(TinyWasmValue::Value128(state.globals.get_128(resolve(global_addrs, *index, "global")?)));
            }
            GlobalGetRef(index) => {
                let value = state.globals.get_32(resolve(global_addrs, *index, "global")?);
                stack.push(TinyWasmValue::ValueRef(ValueRef::from_raw(value)));
            }
            Ref(RefValue::Null) => stack.push(TinyWasmValue::ValueRef(ValueRef::NULL)),
            Ref(RefValue::Func(func)) => {
                let addr = resolve(func_addrs, func.addr(), "function")?;
                stack.push(TinyWasmValue::ValueRef(ValueRef::from_category_addr(addr)));
            }
            Ref(_) => {
                return cold!(Err(Error::other("unsupported reference constant")));
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
                let type_addr = resolve(type_addrs, *type_index, "type")?;
                let fields = state
                    .get_type(type_addr)
                    .as_struct()
                    .ok_or_else(|| Error::other("GC constant type is not a struct"))?
                    .fields
                    .as_ref();
                let default = matches!(instruction, StructNewDefault(_));
                let mut values = Vec::new();
                cold_err!(values.try_reserve_exact(fields.len())).map_err(|_| Trap::OutOfMemory)?;
                if default {
                    values.extend(fields.iter().map(|field| default_value(field.storage)));
                } else {
                    for field in fields.iter().rev() {
                        values.push(pop_value(&mut stack, field.storage)?);
                    }
                    values.reverse();
                }
                alloc_object(state, &mut stack, type_addr, values)?;
            }
            ArrayNew(type_index) | ArrayNewDefault(type_index) => {
                let type_addr = resolve(type_addrs, *type_index, "type")?;
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
                cold_err!(values.try_reserve_exact(len as usize)).map_err(|_| Trap::OutOfMemory)?;
                values.resize(len as usize, value);
                alloc_object(state, &mut stack, type_addr, values)?;
            }
            ArrayNewFixed(type_index, len) => {
                let type_addr = resolve(type_addrs, *type_index, "type")?;
                let storage = state
                    .get_type(type_addr)
                    .as_array()
                    .ok_or_else(|| Error::other("GC constant type is not an array"))?
                    .field
                    .storage;
                let mut values = Vec::new();
                cold_err!(values.try_reserve_exact(*len as usize)).map_err(|_| Trap::OutOfMemory)?;
                for _ in 0..*len {
                    values.push(pop_value(&mut stack, storage)?);
                }
                values.reverse();
                alloc_object(state, &mut stack, type_addr, values)?;
            }
            I32Add | I32Sub | I32Mul => {
                let rhs = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                let lhs = stack.pop().ok_or_else(|| Error::other("const stack underflow"))?;
                let (TinyWasmValue::Value32(lhs), TinyWasmValue::Value32(rhs)) = (lhs, rhs) else {
                    return cold!(Err(Error::other("type mismatch in const i32 op")));
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
                    return cold!(Err(Error::other("type mismatch in const i64 op")));
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
        return cold!(Err(Error::other("empty const expression")));
    };
    if !stack.is_empty() {
        return cold!(Err(Error::other("const expression did not reduce to single value")));
    }
    Ok(value)
}
