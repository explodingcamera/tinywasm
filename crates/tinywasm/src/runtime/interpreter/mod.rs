use alloc::format;
use alloc::{string::ToString, vec::Vec};
use core::ops::{BitAnd, BitOr, BitXor, Neg};
use tinywasm_types::{ElementKind, ValType};

use super::{InterpreterRuntime, Stack};
use crate::runtime::{BlockType, CallFrame, LabelFrame};
use crate::{cold, log, unlikely};
use crate::{Error, FuncContext, ModuleInstance, Result, Store, Trap};

mod macros;
mod traits;
use {macros::*, traits::*};

#[cfg(not(feature = "std"))]
mod no_std_floats;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use no_std_floats::NoStdFloatExt;

impl InterpreterRuntime {
    // #[inline(always)] // a small 2-3% performance improvement in some cases
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut Stack) -> Result<()> {
        // The current call frame, gets updated inside of exec_one
        let mut cf = stack.call_stack.pop()?;

        // The function to execute, gets updated from ExecResult::Call
        let mut current_module = store.get_module_instance_raw(cf.func_instance.1);

        loop {
            match exec_one(&mut cf, stack, store, &current_module) {
                // Continue execution at the new top of the call stack
                Ok(ExecResult::Call) => {
                    cf = stack.call_stack.pop()?;

                    // keeping the pointer seperate from the call frame is about 2% faster
                    // than storing it in the call frame
                    if cf.func_instance.1 != current_module.id() {
                        current_module.swap_with(cf.func_instance.1, store);
                    }
                }

                // return from the function
                Ok(ExecResult::Return) => return Ok(()),

                // continue to the next instruction and increment the instruction pointer
                Ok(ExecResult::Ok) => cf.instr_ptr += 1,

                // trap the program
                Err(error) => {
                    cf.instr_ptr += 1;
                    // push the call frame back onto the stack so that it can be resumed
                    // if the trap can be handled
                    stack.call_stack.push(cf)?;
                    return Err(error);
                }
            }
        }
    }
}

enum ExecResult {
    Ok,
    Return,
    Call,
}

/// Run a single step of the interpreter
/// A seperate function is used so later, we can more easily implement
/// a step-by-step debugger (using generators once they're stable?)
// we want this be always part of the loop, rust just doesn't inline it as its too big
// this can be a 30%+ performance difference in some cases
#[inline(always)]
fn exec_one(cf: &mut CallFrame, stack: &mut Stack, store: &mut Store, module: &ModuleInstance) -> Result<ExecResult> {
    let instrs = &cf.func_instance.0.instructions;
    if unlikely(cf.instr_ptr >= instrs.len() || instrs.is_empty()) {
        cold();
        log::error!("instr_ptr out of bounds: {} >= {}", cf.instr_ptr, instrs.len());
        return Err(Error::Other(format!("instr_ptr out of bounds: {} >= {}", cf.instr_ptr, instrs.len())));
    }

    // A match statement is probably the fastest way to do this without
    // unreasonable complexity
    // See https://pliniker.github.io/post/dispatchers/
    use tinywasm_types::Instruction::*;
    match &instrs[cf.instr_ptr] {
        Nop => { /* do nothing */ }
        Unreachable => {
            cold();
            return Err(crate::Trap::Unreachable.into());
        } // we don't need to include the call frame here because it's already on the stack
        Drop => stack.values.pop().map(|_| ())?,

        Select(
            _valtype, // due to validation, we know that the type of the values on the stack are correct
        ) => {
            // due to validation, we know that the type of the values on the stack
            let cond: i32 = stack.values.pop()?.into();
            let val2 = stack.values.pop()?;

            // if cond != 0, we already have the right value on the stack
            if cond == 0 {
                let _ = stack.values.pop()?;
                stack.values.push(val2);
            }
        }

        Call(v) => {
            // prepare the call frame
            let func_idx = module.resolve_func_addr(*v);
            let func_inst = store.get_func(func_idx as usize)?.clone();

            let wasm_func = match &func_inst.func {
                crate::Function::Wasm(wasm_func) => wasm_func.clone(),
                crate::Function::Host(host_func) => {
                    let func = &host_func.func;
                    let params = stack.values.pop_params(&host_func.ty.params)?;
                    let res = (func)(FuncContext { store, module_addr: module.id() }, &params)?;
                    stack.values.extend_from_typed(&res);
                    return Ok(ExecResult::Ok);
                }
            };

            let params = stack.values.pop_n_rev(wasm_func.ty.params.len())?;
            let call_frame = CallFrame::new(wasm_func, func_inst.owner, params);

            // push the call frame
            cf.instr_ptr += 1; // skip the call instruction
            stack.call_stack.push(cf.clone())?;
            stack.call_stack.push(call_frame)?;

            // call the function
            return Ok(ExecResult::Call);
        }

        CallIndirect(type_addr, table_addr) => {
            let table = store.get_table(module.resolve_table_addr(*table_addr) as usize)?;
            let table_idx = stack.values.pop_t::<u32>()?;

            // verify that the table is of the right type, this should be validated by the parser already
            assert!(table.borrow().kind.element_type == ValType::RefFunc, "table is not of type funcref");

            let func_ref = {
                table
                    .borrow()
                    .get(table_idx as usize)?
                    .addr()
                    .ok_or(Trap::UninitializedElement { index: table_idx as usize })?
            };

            let func_inst = store.get_func(func_ref as usize)?.clone();
            let call_ty = module.func_ty(*type_addr);

            let wasm_func = match func_inst.func {
                crate::Function::Wasm(ref f) => f.clone(),
                crate::Function::Host(host_func) => {
                    if unlikely(host_func.ty != *call_ty) {
                        log::error!("indirect call type mismatch: {:?} != {:?}", host_func.ty, call_ty);
                        return Err(Trap::IndirectCallTypeMismatch {
                            actual: host_func.ty.clone(),
                            expected: call_ty.clone(),
                        }
                        .into());
                    }

                    let host_func = host_func.clone();
                    let params = stack.values.pop_params(&host_func.ty.params)?;
                    let res = (host_func.func)(FuncContext { store, module_addr: module.id() }, &params)?;
                    stack.values.extend_from_typed(&res);
                    return Ok(ExecResult::Ok);
                }
            };

            if unlikely(wasm_func.ty != *call_ty) {
                log::error!("indirect call type mismatch: {:?} != {:?}", wasm_func.ty, call_ty);
                return Err(
                    Trap::IndirectCallTypeMismatch { actual: wasm_func.ty.clone(), expected: call_ty.clone() }.into()
                );
            }

            let params = stack.values.pop_n_rev(wasm_func.ty.params.len())?;
            let call_frame = CallFrame::new(wasm_func, func_inst.owner, params);

            // push the call frame
            cf.instr_ptr += 1; // skip the call instruction
            stack.call_stack.push(cf.clone())?;
            stack.call_stack.push(call_frame)?;

            // call the function
            return Ok(ExecResult::Call);
        }

        If(args, else_offset, end_offset) => {
            // truthy value is on the top of the stack, so enter the then block
            if stack.values.pop_t::<i32>()? != 0 {
                cf.enter_label(
                    LabelFrame::new(
                        cf.instr_ptr,
                        cf.instr_ptr + *end_offset,
                        stack.values.len(), // - params,
                        BlockType::If,
                        args,
                        module,
                    ),
                    &mut stack.values,
                );
                return Ok(ExecResult::Ok);
            }

            // falsy value is on the top of the stack
            if let Some(else_offset) = else_offset {
                let label = LabelFrame::new(
                    cf.instr_ptr + *else_offset,
                    cf.instr_ptr + *end_offset,
                    stack.values.len(), // - params,
                    BlockType::Else,
                    args,
                    module,
                );
                cf.instr_ptr += *else_offset;
                cf.enter_label(label, &mut stack.values);
            } else {
                cf.instr_ptr += *end_offset;
            }
        }

        Loop(args, end_offset) => {
            cf.enter_label(
                LabelFrame::new(
                    cf.instr_ptr,
                    cf.instr_ptr + *end_offset,
                    stack.values.len(), // - params,
                    BlockType::Loop,
                    args,
                    module,
                ),
                &mut stack.values,
            );
        }

        Block(args, end_offset) => {
            cf.enter_label(
                LabelFrame::new(
                    cf.instr_ptr,
                    cf.instr_ptr + *end_offset,
                    stack.values.len(), // - params,
                    BlockType::Block,
                    args,
                    module,
                ),
                &mut stack.values,
            );
        }

        BrTable(default, len) => {
            let instr = instrs[cf.instr_ptr + 1..cf.instr_ptr + 1 + *len]
                .iter()
                .map(|i| match i {
                    BrLabel(l) => Ok(*l),
                    _ => {
                        cold();
                        panic!("Expected BrLabel, this should have been validated by the parser")
                    }
                })
                .collect::<Result<Vec<_>>>()?;

            if unlikely(instr.len() != *len) {
                panic!(
                    "Expected {} BrLabel instructions, got {}, this should have been validated by the parser",
                    len,
                    instr.len()
                );
            }

            let idx = stack.values.pop_t::<i32>()? as usize;
            let to = instr.get(idx).unwrap_or(default);
            break_to!(cf, stack, to);
        }

        Br(v) => break_to!(cf, stack, v),
        BrIf(v) => {
            if stack.values.pop_t::<i32>()? != 0 {
                break_to!(cf, stack, v);
            }
        }

        Return => match stack.call_stack.is_empty() {
            true => return Ok(ExecResult::Return),
            false => return Ok(ExecResult::Call),
        },

        EndFunc => {
            assert!(
                cf.labels.len() == 0,
                "endfunc: block frames not empty, this should have been validated by the parser"
            );

            match stack.call_stack.is_empty() {
                true => return Ok(ExecResult::Return),
                false => return Ok(ExecResult::Call),
            }
        }

        // We're essentially using else as a EndBlockFrame instruction for if blocks
        Else(end_offset) => {
            let Some(block) = cf.labels.pop() else {
                cold();
                panic!("else: no label to end, this should have been validated by the parser");
            };

            let res_count = block.results;
            stack.values.truncate_keep(block.stack_ptr, res_count);
            cf.instr_ptr += *end_offset;
        }

        EndBlockFrame => {
            // remove the label from the label stack
            let Some(block) = cf.labels.pop() else {
                cold();
                panic!("end: no label to end, this should have been validated by the parser");
            };
            stack.values.truncate_keep(block.stack_ptr, block.results)
        }

        LocalGet(local_index) => stack.values.push(cf.get_local(*local_index as usize)),
        LocalSet(local_index) => cf.set_local(*local_index as usize, stack.values.pop()?),
        LocalTee(local_index) => cf.set_local(*local_index as usize, *stack.values.last()?),

        GlobalGet(global_index) => {
            let idx = module.resolve_global_addr(*global_index);
            let global = store.get_global_val(idx as usize)?;
            stack.values.push(global);
        }

        GlobalSet(global_index) => {
            let idx = module.resolve_global_addr(*global_index);
            store.set_global_val(idx as usize, stack.values.pop()?)?;
        }

        I32Const(val) => stack.values.push((*val).into()),
        I64Const(val) => stack.values.push((*val).into()),
        F32Const(val) => stack.values.push((*val).into()),
        F64Const(val) => stack.values.push((*val).into()),

        MemorySize(addr, byte) => {
            if *byte != 0 {
                cold();
                return Err(Error::UnsupportedFeature("memory.size with byte != 0".to_string()));
            }

            let mem_idx = module.resolve_mem_addr(*addr);
            let mem = store.get_mem(mem_idx as usize)?;
            stack.values.push((mem.borrow().page_count() as i32).into());
        }

        MemoryGrow(addr, byte) => {
            if *byte != 0 {
                cold();
                return Err(Error::UnsupportedFeature("memory.grow with byte != 0".to_string()));
            }

            let mem_idx = module.resolve_mem_addr(*addr);
            let mem = store.get_mem(mem_idx as usize)?;

            let (res, prev_size) = {
                let mut mem = mem.borrow_mut();
                let prev_size = mem.page_count() as i32;
                (mem.grow(stack.values.pop_t::<i32>()?), prev_size)
            };

            match res {
                Some(_) => stack.values.push(prev_size.into()),
                None => stack.values.push((-1).into()),
            }
        }

        // Bulk memory operations
        MemoryCopy(from, to) => {
            let size = stack.values.pop_t::<i32>()?;
            let src = stack.values.pop_t::<i32>()?;
            let dst = stack.values.pop_t::<i32>()?;

            let mem = store.get_mem(module.resolve_mem_addr(*from) as usize)?;
            let mut mem = mem.borrow_mut();

            if from == to {
                // copy within the same memory
                mem.copy_within(dst as usize, src as usize, size as usize)?;
            } else {
                // copy between two memories
                let mem2 = store.get_mem(module.resolve_mem_addr(*to) as usize)?;
                let mut mem2 = mem2.borrow_mut();
                mem2.copy_from_slice(dst as usize, mem.load(src as usize, 0, size as usize)?)?;
            }
        }

        MemoryFill(addr) => {
            let size = stack.values.pop_t::<i32>()?;
            let val = stack.values.pop_t::<i32>()?;
            let dst = stack.values.pop_t::<i32>()?;

            let mem = store.get_mem(module.resolve_mem_addr(*addr) as usize)?;
            let mut mem = mem.borrow_mut();
            mem.fill(dst as usize, size as usize, val as u8)?;
        }

        MemoryInit(data_index, mem_index) => {
            let size = stack.values.pop_t::<i32>()? as usize;
            let offset = stack.values.pop_t::<i32>()? as usize;
            let dst = stack.values.pop_t::<i32>()? as usize;

            let data_idx = module.resolve_data_addr(*data_index);
            let Some(ref data) = store.get_data(data_idx as usize)?.data else {
                cold();
                return Err(Trap::MemoryOutOfBounds { offset: 0, len: 0, max: 0 }.into());
            };

            let mem_idx = module.resolve_mem_addr(*mem_index);
            let mem = store.get_mem(mem_idx as usize)?;

            let data_len = data.len();
            if offset + size > data_len {
                cold();
                return Err(Trap::MemoryOutOfBounds { offset, len: size, max: data_len }.into());
            }

            let mut mem = mem.borrow_mut();
            let data = &data[offset..(offset + size)];

            // mem.store checks bounds
            mem.store(dst, 0, data, size)?;
        }

        DataDrop(data_index) => {
            let data_idx = module.resolve_data_addr(*data_index);
            let data = store.get_data_mut(data_idx as usize)?;
            data.drop();
        }

        I32Store(arg) => mem_store!(i32, arg, stack, store, module),
        I64Store(arg) => mem_store!(i64, arg, stack, store, module),
        F32Store(arg) => mem_store!(f32, arg, stack, store, module),
        F64Store(arg) => mem_store!(f64, arg, stack, store, module),
        I32Store8(arg) => mem_store!(i8, i32, arg, stack, store, module),
        I32Store16(arg) => mem_store!(i16, i32, arg, stack, store, module),
        I64Store8(arg) => mem_store!(i8, i64, arg, stack, store, module),
        I64Store16(arg) => mem_store!(i16, i64, arg, stack, store, module),
        I64Store32(arg) => mem_store!(i32, i64, arg, stack, store, module),

        I32Load(arg) => mem_load!(i32, arg, stack, store, module),
        I64Load(arg) => mem_load!(i64, arg, stack, store, module),
        F32Load(arg) => mem_load!(f32, arg, stack, store, module),
        F64Load(arg) => mem_load!(f64, arg, stack, store, module),
        I32Load8S(arg) => mem_load!(i8, i32, arg, stack, store, module),
        I32Load8U(arg) => mem_load!(u8, i32, arg, stack, store, module),
        I32Load16S(arg) => mem_load!(i16, i32, arg, stack, store, module),
        I32Load16U(arg) => mem_load!(u16, i32, arg, stack, store, module),
        I64Load8S(arg) => mem_load!(i8, i64, arg, stack, store, module),
        I64Load8U(arg) => mem_load!(u8, i64, arg, stack, store, module),
        I64Load16S(arg) => mem_load!(i16, i64, arg, stack, store, module),
        I64Load16U(arg) => mem_load!(u16, i64, arg, stack, store, module),
        I64Load32S(arg) => mem_load!(i32, i64, arg, stack, store, module),
        I64Load32U(arg) => mem_load!(u32, i64, arg, stack, store, module),

        I64Eqz => comp_zero!(==, i64, stack),
        I32Eqz => comp_zero!(==, i32, stack),

        I32Eq => comp!(==, i32, stack),
        I64Eq => comp!(==, i64, stack),
        F32Eq => comp!(==, f32, stack),
        F64Eq => comp!(==, f64, stack),

        I32Ne => comp!(!=, i32, stack),
        I64Ne => comp!(!=, i64, stack),
        F32Ne => comp!(!=, f32, stack),
        F64Ne => comp!(!=, f64, stack),

        I32LtS => comp!(<, i32, stack),
        I64LtS => comp!(<, i64, stack),
        I32LtU => comp!(<, i32, u32, stack),
        I64LtU => comp!(<, i64, u64, stack),
        F32Lt => comp!(<, f32, stack),
        F64Lt => comp!(<, f64, stack),

        I32LeS => comp!(<=, i32, stack),
        I64LeS => comp!(<=, i64, stack),
        I32LeU => comp!(<=, i32, u32, stack),
        I64LeU => comp!(<=, i64, u64, stack),
        F32Le => comp!(<=, f32, stack),
        F64Le => comp!(<=, f64, stack),

        I32GeS => comp!(>=, i32, stack),
        I64GeS => comp!(>=, i64, stack),
        I32GeU => comp!(>=, i32, u32, stack),
        I64GeU => comp!(>=, i64, u64, stack),
        F32Ge => comp!(>=, f32, stack),
        F64Ge => comp!(>=, f64, stack),

        I32GtS => comp!(>, i32, stack),
        I64GtS => comp!(>, i64, stack),
        I32GtU => comp!(>, i32, u32, stack),
        I64GtU => comp!(>, i64, u64, stack),
        F32Gt => comp!(>, f32, stack),
        F64Gt => comp!(>, f64, stack),

        I64Add => arithmetic!(wrapping_add, i64, stack),
        I32Add => arithmetic!(wrapping_add, i32, stack),
        F32Add => arithmetic!(+, f32, stack),
        F64Add => arithmetic!(+, f64, stack),

        I32Sub => arithmetic!(wrapping_sub, i32, stack),
        I64Sub => arithmetic!(wrapping_sub, i64, stack),
        F32Sub => arithmetic!(-, f32, stack),
        F64Sub => arithmetic!(-, f64, stack),

        F32Div => arithmetic!(/, f32, stack),
        F64Div => arithmetic!(/, f64, stack),

        I32Mul => arithmetic!(wrapping_mul, i32, stack),
        I64Mul => arithmetic!(wrapping_mul, i64, stack),
        F32Mul => arithmetic!(*, f32, stack),
        F64Mul => arithmetic!(*, f64, stack),

        // these can trap
        I32DivS => checked_int_arithmetic!(checked_div, i32, stack),
        I64DivS => checked_int_arithmetic!(checked_div, i64, stack),
        I32DivU => checked_int_arithmetic!(checked_div, i32, u32, stack),
        I64DivU => checked_int_arithmetic!(checked_div, i64, u64, stack),

        I32RemS => checked_int_arithmetic!(checked_wrapping_rem, i32, stack),
        I64RemS => checked_int_arithmetic!(checked_wrapping_rem, i64, stack),
        I32RemU => checked_int_arithmetic!(checked_wrapping_rem, i32, u32, stack),
        I64RemU => checked_int_arithmetic!(checked_wrapping_rem, i64, u64, stack),

        I32And => arithmetic!(bitand, i32, stack),
        I64And => arithmetic!(bitand, i64, stack),
        I32Or => arithmetic!(bitor, i32, stack),
        I64Or => arithmetic!(bitor, i64, stack),
        I32Xor => arithmetic!(bitxor, i32, stack),
        I64Xor => arithmetic!(bitxor, i64, stack),
        I32Shl => arithmetic!(wasm_shl, i32, stack),
        I64Shl => arithmetic!(wasm_shl, i64, stack),
        I32ShrS => arithmetic!(wasm_shr, i32, stack),
        I64ShrS => arithmetic!(wasm_shr, i64, stack),
        I32ShrU => arithmetic!(wasm_shr, u32, i32, stack),
        I64ShrU => arithmetic!(wasm_shr, u64, i64, stack),
        I32Rotl => arithmetic!(wasm_rotl, i32, stack),
        I64Rotl => arithmetic!(wasm_rotl, i64, stack),
        I32Rotr => arithmetic!(wasm_rotr, i32, stack),
        I64Rotr => arithmetic!(wasm_rotr, i64, stack),

        I32Clz => arithmetic_single!(leading_zeros, i32, stack),
        I64Clz => arithmetic_single!(leading_zeros, i64, stack),
        I32Ctz => arithmetic_single!(trailing_zeros, i32, stack),
        I64Ctz => arithmetic_single!(trailing_zeros, i64, stack),
        I32Popcnt => arithmetic_single!(count_ones, i32, stack),
        I64Popcnt => arithmetic_single!(count_ones, i64, stack),

        F32ConvertI32S => conv!(i32, f32, stack),
        F32ConvertI64S => conv!(i64, f32, stack),
        F64ConvertI32S => conv!(i32, f64, stack),
        F64ConvertI64S => conv!(i64, f64, stack),
        F32ConvertI32U => conv!(i32, u32, f32, stack),
        F32ConvertI64U => conv!(i64, u64, f32, stack),
        F64ConvertI32U => conv!(i32, u32, f64, stack),
        F64ConvertI64U => conv!(i64, u64, f64, stack),
        I32Extend8S => conv!(i32, i8, i32, stack),
        I32Extend16S => conv!(i32, i16, i32, stack),
        I64Extend8S => conv!(i64, i8, i64, stack),
        I64Extend16S => conv!(i64, i16, i64, stack),
        I64Extend32S => conv!(i64, i32, i64, stack),
        I64ExtendI32U => conv!(i32, u32, i64, stack),
        I64ExtendI32S => conv!(i32, i64, stack),
        I32WrapI64 => conv!(i64, i32, stack),

        F32DemoteF64 => conv!(f64, f32, stack),
        F64PromoteF32 => conv!(f32, f64, stack),

        F32Abs => arithmetic_single!(abs, f32, stack),
        F64Abs => arithmetic_single!(abs, f64, stack),
        F32Neg => arithmetic_single!(neg, f32, stack),
        F64Neg => arithmetic_single!(neg, f64, stack),
        F32Ceil => arithmetic_single!(ceil, f32, stack),
        F64Ceil => arithmetic_single!(ceil, f64, stack),
        F32Floor => arithmetic_single!(floor, f32, stack),
        F64Floor => arithmetic_single!(floor, f64, stack),
        F32Trunc => arithmetic_single!(trunc, f32, stack),
        F64Trunc => arithmetic_single!(trunc, f64, stack),
        F32Nearest => arithmetic_single!(tw_nearest, f32, stack),
        F64Nearest => arithmetic_single!(tw_nearest, f64, stack),
        F32Sqrt => arithmetic_single!(sqrt, f32, stack),
        F64Sqrt => arithmetic_single!(sqrt, f64, stack),
        F32Min => arithmetic!(tw_minimum, f32, stack),
        F64Min => arithmetic!(tw_minimum, f64, stack),
        F32Max => arithmetic!(tw_maximum, f32, stack),
        F64Max => arithmetic!(tw_maximum, f64, stack),
        F32Copysign => arithmetic!(copysign, f32, stack),
        F64Copysign => arithmetic!(copysign, f64, stack),

        // no-op instructions since types are erased at runtime
        I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64 => {}

        // unsigned versions of these are a bit broken atm
        I32TruncF32S => checked_conv_float!(f32, i32, stack),
        I32TruncF64S => checked_conv_float!(f64, i32, stack),
        I32TruncF32U => checked_conv_float!(f32, u32, i32, stack),
        I32TruncF64U => checked_conv_float!(f64, u32, i32, stack),
        I64TruncF32S => checked_conv_float!(f32, i64, stack),
        I64TruncF64S => checked_conv_float!(f64, i64, stack),
        I64TruncF32U => checked_conv_float!(f32, u64, i64, stack),
        I64TruncF64U => checked_conv_float!(f64, u64, i64, stack),

        TableGet(table_index) => {
            let table_idx = module.resolve_table_addr(*table_index);
            let table = store.get_table(table_idx as usize)?;
            let idx = stack.values.pop_t::<i32>()? as usize;
            let v = table.borrow().get_wasm_val(idx)?;
            stack.values.push(v.into());
        }

        TableSet(table_index) => {
            let table_idx = module.resolve_table_addr(*table_index);
            let table = store.get_table(table_idx as usize)?;
            let val = stack.values.pop_t::<u32>()?;
            let idx = stack.values.pop_t::<u32>()? as usize;
            table.borrow_mut().set(idx, val)?;
        }

        TableSize(table_index) => {
            let table_idx = module.resolve_table_addr(*table_index);
            let table = store.get_table(table_idx as usize)?;
            stack.values.push(table.borrow().size().into());
        }

        TableInit(table_index, elem_index) => {
            let table_idx = module.resolve_table_addr(*table_index);
            let table = store.get_table(table_idx as usize)?;

            let elem_idx = module.resolve_elem_addr(*elem_index);
            let elem = store.get_elem(elem_idx as usize)?;

            if let ElementKind::Passive = elem.kind {
                return Err(Trap::TableOutOfBounds { offset: 0, len: 0, max: 0 }.into());
            }

            let Some(items) = elem.items.as_ref() else {
                return Err(Trap::TableOutOfBounds { offset: 0, len: 0, max: 0 }.into());
            };

            table.borrow_mut().init(module.func_addrs(), 0, items)?;
        }

        I32TruncSatF32S => arithmetic_single!(trunc, f32, i32, stack),
        I32TruncSatF32U => arithmetic_single!(trunc, f32, u32, stack),
        I32TruncSatF64S => arithmetic_single!(trunc, f64, i32, stack),
        I32TruncSatF64U => arithmetic_single!(trunc, f64, u32, stack),
        I64TruncSatF32S => arithmetic_single!(trunc, f32, i64, stack),
        I64TruncSatF32U => arithmetic_single!(trunc, f32, u64, stack),
        I64TruncSatF64S => arithmetic_single!(trunc, f64, i64, stack),
        I64TruncSatF64U => arithmetic_single!(trunc, f64, u64, stack),

        i => {
            cold();
            log::error!("unimplemented instruction: {:?}", i);
            return Err(Error::UnsupportedFeature(alloc::format!("unimplemented instruction: {:?}", i)));
        }
    };

    Ok(ExecResult::Ok)
}
