use super::{InterpreterRuntime, Stack};
use crate::log;
use crate::{
    log::debug,
    runtime::{BlockType, CallFrame, LabelArgs, LabelFrame},
    Error, FuncContext, ModuleInstance, Result, Store, Trap,
};
use alloc::{string::ToString, vec::Vec};
use core::ops::{BitAnd, BitOr, BitXor, Neg};
use tinywasm_types::{ElementKind, Instruction, ValType};

#[cfg(not(feature = "std"))]
mod no_std_floats;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use no_std_floats::FExt;

mod macros;
mod traits;

use macros::*;
use traits::*;

impl InterpreterRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut Stack) -> Result<()> {
        // The current call frame, gets updated inside of exec_one
        let mut cf = stack.call_stack.pop()?;

        let mut func_inst = cf.func_instance.clone();
        let mut wasm_func = func_inst.assert_wasm().expect("exec expected wasm function");

        // The function to execute, gets updated from ExecResult::Call
        let mut instrs = &wasm_func.instructions;

        let mut current_module = store.get_module_instance(func_inst.owner).unwrap().clone();

        while let Some(instr) = instrs.get(cf.instr_ptr) {
            match exec_one(&mut cf, instr, instrs, stack, store, &current_module)? {
                // Continue execution at the new top of the call stack
                ExecResult::Call => {
                    cf = stack.call_stack.pop()?;
                    func_inst = cf.func_instance.clone();
                    wasm_func = func_inst.assert_wasm().expect("exec expected wasm function");
                    instrs = &wasm_func.instructions;

                    if cf.func_instance.owner != current_module.id() {
                        current_module.swap(
                            store
                                .get_module_instance(cf.func_instance.owner)
                                .unwrap_or_else(|| {
                                    panic!(
                                        "exec expected module instance {} to exist for function",
                                        cf.func_instance.owner
                                    )
                                })
                                .clone(),
                        );
                    }

                    continue;
                }

                // return from the function
                ExecResult::Return => return Ok(()),

                // continue to the next instruction and increment the instruction pointer
                ExecResult::Ok => {
                    cf.instr_ptr += 1;
                }

                // trap the program
                ExecResult::Trap(trap) => {
                    cf.instr_ptr += 1;
                    // push the call frame back onto the stack so that it can be resumed
                    // if the trap can be handled
                    stack.call_stack.push(cf)?;
                    return Err(Error::Trap(trap));
                }
            }
        }

        debug!("end of exec");
        debug!("stack: {:?}", stack.values);
        debug!("insts: {:?}", instrs);
        debug!("instr_ptr: {}", cf.instr_ptr);
        Err(Error::FuncDidNotReturn)
    }
}

enum ExecResult {
    Ok,
    Return,
    Call,
    Trap(crate::Trap),
}

// Break to a block at the given index (relative to the current frame)
// If there is no block at the given index, return or call the parent function
//
// This is a bit hard to see from the spec, but it's vaild to use breaks to return
// from a function, so we need to check if the label stack is empty
macro_rules! break_to {
    ($cf:ident, $stack:ident, $break_to_relative:ident) => {{
        if $cf.break_to(*$break_to_relative, &mut $stack.values).is_none() {
            if $stack.call_stack.is_empty() {
                return Ok(ExecResult::Return);
            } else {
                return Ok(ExecResult::Call);
            }
        }
    }};
}

/// Run a single step of the interpreter
/// A seperate function is used so later, we can more easily implement
/// a step-by-step debugger (using generators once they're stable?)
#[inline]
fn exec_one(
    cf: &mut CallFrame,
    instr: &Instruction,
    instrs: &[Instruction],
    stack: &mut Stack,
    store: &mut Store,
    module: &ModuleInstance,
) -> Result<ExecResult> {
    debug!("ptr: {} instr: {:?}", cf.instr_ptr, instr);

    use tinywasm_types::Instruction::*;
    match instr {
        Nop => { /* do nothing */ }
        Unreachable => return Ok(ExecResult::Trap(crate::Trap::Unreachable)), // we don't need to include the call frame here because it's already on the stack
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

            let (locals, ty) = match &func_inst.func {
                crate::Function::Wasm(ref f) => (f.locals.to_vec(), f.ty.clone()),
                crate::Function::Host(host_func) => {
                    let func = host_func.func.clone();
                    let params = stack.values.pop_params(&host_func.ty.params)?;
                    let res = (func)(FuncContext { store, module }, &params)?;
                    stack.values.extend_from_typed(&res);
                    return Ok(ExecResult::Ok);
                }
            };

            let params = stack.values.pop_n_rev(ty.params.len())?;
            let call_frame = CallFrame::new_raw(func_inst, &params, locals);

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
            let func_ty = func_inst.func.ty();

            log::info!("type_addr: {}", type_addr);
            log::info!("types: {:?}", module.func_tys());
            let call_ty = module.func_ty(*type_addr);

            log::info!("call_indirect: current fn owner: {:?}", module.id());
            log::info!("call_indirect: func owner: {:?}", func_inst.owner);

            if func_ty != call_ty {
                log::error!("indirect call type mismatch: {:?} != {:?}", func_ty, call_ty);
                return Err(
                    Trap::IndirectCallTypeMismatch { actual: func_ty.clone(), expected: call_ty.clone() }.into()
                );
            }

            let locals = match &func_inst.func {
                crate::Function::Wasm(ref f) => f.locals.to_vec(),
                crate::Function::Host(host_func) => {
                    let func = host_func.func.clone();
                    let params = stack.values.pop_params(&func_ty.params)?;
                    let res = (func)(FuncContext { store, module }, &params)?;
                    stack.values.extend_from_typed(&res);
                    return Ok(ExecResult::Ok);
                }
            };

            let params = stack.values.pop_n_rev(func_ty.params.len())?;
            let call_frame = CallFrame::new_raw(func_inst, &params, locals);

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
                log::trace!("entering then");
                cf.enter_label(
                    LabelFrame {
                        instr_ptr: cf.instr_ptr,
                        end_instr_ptr: cf.instr_ptr + *end_offset,
                        stack_ptr: stack.values.len(), // - params,
                        args: LabelArgs::new(*args, module)?,
                        ty: BlockType::If,
                    },
                    &mut stack.values,
                );
                return Ok(ExecResult::Ok);
            }

            // falsy value is on the top of the stack
            if let Some(else_offset) = else_offset {
                log::debug!("entering else at {}", cf.instr_ptr + *else_offset);
                cf.enter_label(
                    LabelFrame {
                        instr_ptr: cf.instr_ptr + *else_offset,
                        end_instr_ptr: cf.instr_ptr + *end_offset,
                        stack_ptr: stack.values.len(), // - params,
                        args: LabelArgs::new(*args, module)?,
                        ty: BlockType::Else,
                    },
                    &mut stack.values,
                );
                cf.instr_ptr += *else_offset;
            } else {
                cf.instr_ptr += *end_offset;
            }
        }

        Loop(args, end_offset) => {
            // let params = stack.values.pop_block_params(*args, &module)?;
            cf.enter_label(
                LabelFrame {
                    instr_ptr: cf.instr_ptr,
                    end_instr_ptr: cf.instr_ptr + *end_offset,
                    stack_ptr: stack.values.len(), // - params,
                    args: LabelArgs::new(*args, module)?,
                    ty: BlockType::Loop,
                },
                &mut stack.values,
            );
        }

        Block(args, end_offset) => {
            cf.enter_label(
                LabelFrame {
                    instr_ptr: cf.instr_ptr,
                    end_instr_ptr: cf.instr_ptr + *end_offset,
                    stack_ptr: stack.values.len(), //- params,
                    args: LabelArgs::new(*args, module)?,
                    ty: BlockType::Block,
                },
                &mut stack.values,
            );
        }

        BrTable(default, len) => {
            let instr = instrs[cf.instr_ptr + 1..cf.instr_ptr + 1 + *len]
                .iter()
                .map(|i| match i {
                    BrLabel(l) => Ok(*l),
                    _ => panic!("Expected BrLabel, this should have been validated by the parser"),
                })
                .collect::<Result<Vec<_>>>()?;

            if instr.len() != *len {
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
                panic!("else: no label to end, this should have been validated by the parser");
            };

            let res_count = block.args.results;
            stack.values.truncate_keep(block.stack_ptr, res_count);
            cf.instr_ptr += *end_offset;
        }

        EndBlockFrame => {
            // remove the label from the label stack
            let Some(block) = cf.labels.pop() else {
                panic!("end: no label to end, this should have been validated by the parser");
            };
            stack.values.truncate_keep(block.stack_ptr, block.args.results)
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
                unimplemented!("memory.size with byte != 0");
            }

            let mem_idx = module.resolve_mem_addr(*addr);
            let mem = store.get_mem(mem_idx as usize)?;
            stack.values.push((mem.borrow().page_count() as i32).into());
        }

        MemoryGrow(addr, byte) => {
            if *byte != 0 {
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
        F32Nearest => arithmetic_single!(wasm_nearest, f32, stack),
        F64Nearest => arithmetic_single!(wasm_nearest, f64, stack),
        F32Sqrt => arithmetic_single!(sqrt, f32, stack),
        F64Sqrt => arithmetic_single!(sqrt, f64, stack),
        F32Min => arithmetic!(wasm_min, f32, stack),
        F64Min => arithmetic!(wasm_min, f64, stack),
        F32Max => arithmetic!(wasm_max, f32, stack),
        F64Max => arithmetic!(wasm_max, f64, stack),
        F32Copysign => arithmetic!(copysign, f32, stack),
        F64Copysign => arithmetic!(copysign, f64, stack),

        // no-op instructions since types are erased at runtime
        I32ReinterpretF32 => {}
        I64ReinterpretF64 => {}
        F32ReinterpretI32 => {}
        F64ReinterpretI64 => {}

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

            if elem.kind != ElementKind::Passive {
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
            log::error!("unimplemented instruction: {:?}", i);
            return Err(Error::UnsupportedFeature(alloc::format!("unimplemented instruction: {:?}", i)));
        }
    };

    Ok(ExecResult::Ok)
}
