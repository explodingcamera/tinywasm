use core::ops::{BitAnd, BitOr, BitXor, Neg};

use super::{DefaultRuntime, Stack};
use crate::{
    get_label_args,
    log::debug,
    runtime::{BlockType, LabelFrame},
    CallFrame, Error, ModuleInstance, Result, Store,
};
use alloc::{format, vec::Vec};
use log::info;
use tinywasm_types::Instruction;

mod macros;
mod traits;
use macros::*;
use traits::*;

impl DefaultRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut Stack, module: ModuleInstance) -> Result<()> {
        log::info!("exports: {:?}", module.exports());
        log::info!("func_addrs: {:?}", module.func_addrs());
        log::info!("func_ty_addrs: {:?}", module.func_ty_addrs().len());
        log::info!("store funcs: {:?}", store.data.funcs.len());

        // The current call frame, gets updated inside of exec_one
        let mut cf = stack.call_stack.pop()?;

        // The function to execute, gets updated from ExecResult::Call
        let mut func = store.get_func(cf.func_ptr)?.clone();
        let mut instrs = func.instructions();

        // TODO: we might be able to index into the instructions directly
        // since the instruction pointer should always be in bounds
        while let Some(instr) = instrs.get(cf.instr_ptr) {
            match exec_one(&mut cf, instr, instrs, stack, store, &module)? {
                // Continue execution at the new top of the call stack
                ExecResult::Call => {
                    func = store.get_func(cf.func_ptr)?.clone();
                    instrs = func.instructions();
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
                    stack.call_stack.push(cf);
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
    info!("ptr: {} instr: {:?}", cf.instr_ptr, instr);

    use tinywasm_types::Instruction::*;
    match instr {
        Nop => { /* do nothing */ }
        Unreachable => return Ok(ExecResult::Trap(crate::Trap::Unreachable)), // we don't need to include the call frame here because it's already on the stack
        Drop => stack.values.pop().map(|_| ())?,
        Select(t) => {
            if t.is_some() {
                unimplemented!("select with type");
            }

            let cond: i32 = stack.values.pop()?.into();
            let val2 = stack.values.pop()?;

            // if cond != 0, we already have the right value on the stack
            if cond == 0 {
                let _ = stack.values.pop()?;
                stack.values.push(val2);
            }
        }
        Call(v) => {
            debug!("start call");
            // prepare the call frame
            let func_idx = module.resolve_func_addr(*v);
            let func = store.get_func(func_idx as usize)?;
            let func_ty = module.func_ty(func.ty_addr());

            debug!("params: {:?}", func_ty.params);
            debug!("stack: {:?}", stack.values);
            let params = stack.values.pop_n(func_ty.params.len())?;
            let call_frame = CallFrame::new_raw(*v as usize, &params, func.locals().to_vec());

            // push the call frame
            cf.instr_ptr += 1; // skip the call instruction
            stack.call_stack.push(cf.clone());
            stack.call_stack.push(call_frame);

            // call the function
            *cf = stack.call_stack.pop()?;
            debug!("calling: {:?}", func);
            return Ok(ExecResult::Call);
        }

        If(args, else_offset, end_offset) => {
            let end_instr_ptr = cf.instr_ptr + *end_offset;

            info!(
                "if it's true, we'll jump to the next instruction (@{})",
                cf.instr_ptr + 1
            );

            if let Some(else_offset) = else_offset {
                info!(
                    "else: {:?} (@{})",
                    instrs[cf.instr_ptr + else_offset],
                    cf.instr_ptr + else_offset
                );
            };

            info!("end: {:?} (@{})", instrs[end_instr_ptr], end_instr_ptr);

            if stack.values.pop_t::<i32>()? != 0 {
                cf.enter_label(
                    LabelFrame {
                        instr_ptr: cf.instr_ptr,
                        end_instr_ptr: cf.instr_ptr + *end_offset,
                        stack_ptr: stack.values.len(), // - params,
                        args: get_label_args(*args, module)?,
                        ty: BlockType::If,
                    },
                    &mut stack.values,
                )
            }
        }

        Loop(args, end_offset) => {
            // let params = stack.values.pop_block_params(*args, &module)?;
            cf.enter_label(
                LabelFrame {
                    instr_ptr: cf.instr_ptr,
                    end_instr_ptr: cf.instr_ptr + *end_offset,
                    stack_ptr: stack.values.len(), // - params,
                    args: get_label_args(*args, module)?,
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
                    args: get_label_args(*args, module)?,
                    ty: BlockType::Block,
                },
                &mut stack.values,
            );
        }

        BrTable(_default, len) => {
            let instr = instrs[cf.instr_ptr + 1..cf.instr_ptr + 1 + *len]
                .iter()
                .map(|i| match i {
                    BrLabel(l) => Ok(*l),
                    _ => panic!("Expected BrLabel, this should have been validated by the parser"),
                })
                .collect::<Result<Vec<_>>>()?;

            if instr.len() != *len {
                panic!("Expected {} BrLabel instructions, got {}", len, instr.len());
            }

            todo!("br_table");
        }

        Br(v) => cf.break_to(*v, &mut stack.values)?,
        BrIf(v) => {
            if stack.values.pop_t::<i32>()? > 0 {
                cf.break_to(*v, &mut stack.values)?
            };
        }

        Return => match stack.call_stack.is_empty() {
            true => return Ok(ExecResult::Return),
            false => {
                *cf = stack.call_stack.pop()?;
                return Ok(ExecResult::Call);
            }
        },

        EndFunc => {
            if cf.labels.len() > 0 {
                panic!("endfunc: block frames not empty, this should have been validated by the parser");
            }

            match stack.call_stack.is_empty() {
                true => return Ok(ExecResult::Return),
                false => {
                    *cf = stack.call_stack.pop()?;
                    return Ok(ExecResult::Call);
                }
            }
        }

        EndBlockFrame => {
            let blocks = &mut cf.labels;

            // remove the label from the label stack
            let Some(block) = blocks.pop() else {
                panic!("end: no label to end, this should have been validated by the parser");
            };

            let res_count = block.args.results;
            info!("we want to keep {} values on the stack", res_count);
            info!("current block stack ptr: {}", block.stack_ptr);
            info!("stack: {:?}", stack.values);

            // trim the lable's stack from the stack
            stack.values.truncate_keep(block.stack_ptr, res_count)
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
            stack.values.push(mem.borrow().size().into());
        }

        MemoryGrow(addr, byte) => {
            if *byte != 0 {
                unimplemented!("memory.grow with byte != 0");
            }

            let mem_idx = module.resolve_mem_addr(*addr);
            let mem = store.get_mem(mem_idx as usize)?;

            let (res, prev_size) = {
                let mut mem = mem.borrow_mut();
                let prev_size = mem.size();
                let new_size = prev_size + stack.values.pop_t::<i32>()?;
                (mem.grow(new_size), prev_size)
            };

            match res {
                Ok(_) => stack.values.push(prev_size.into()),
                Err(_) => stack.values.push((-1).into()),
            }
        }

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
        I32LtU => comp_cast!(<, i32, u32, stack),
        I64LtU => comp_cast!(<, i64, u64, stack),
        F32Lt => comp!(<, f32, stack),
        F64Lt => comp!(<, f64, stack),

        I32LeS => comp!(<=, i32, stack),
        I64LeS => comp!(<=, i64, stack),
        I32LeU => comp_cast!(<=, i32, u32, stack),
        I64LeU => comp_cast!(<=, i64, u64, stack),
        F32Le => comp!(<=, f32, stack),
        F64Le => comp!(<=, f64, stack),

        I32GeS => comp!(>=, i32, stack),
        I64GeS => comp!(>=, i64, stack),
        I32GeU => comp_cast!(>=, i32, u32, stack),
        I64GeU => comp_cast!(>=, i64, u64, stack),
        F32Ge => comp!(>=, f32, stack),
        F64Ge => comp!(>=, f64, stack),

        I32GtS => comp!(>, i32, stack),
        I64GtS => comp!(>, i64, stack),
        I32GtU => comp_cast!(>, i32, u32, stack),
        I64GtU => comp_cast!(>, i64, u64, stack),
        F32Gt => comp!(>, f32, stack),
        F64Gt => comp!(>, f64, stack),

        I64Add => arithmetic_method!(wrapping_add, i64, stack),
        I32Add => arithmetic_method!(wrapping_add, i32, stack),
        F32Add => arithmetic_op!(+, f32, stack),
        F64Add => arithmetic_op!(+, f64, stack),

        I32Sub => arithmetic_method!(wrapping_sub, i32, stack),
        I64Sub => arithmetic_method!(wrapping_sub, i64, stack),
        F32Sub => arithmetic_op!(-, f32, stack),
        F64Sub => arithmetic_op!(-, f64, stack),

        F32Div => arithmetic_op!(/, f32, stack),
        F64Div => arithmetic_op!(/, f64, stack),

        I32Mul => arithmetic_method!(wrapping_mul, i32, stack),
        I64Mul => arithmetic_method!(wrapping_mul, i64, stack),
        F32Mul => arithmetic_op!(*, f32, stack),
        F64Mul => arithmetic_op!(*, f64, stack),

        // these can trap
        I32DivS => checked_arithmetic_method!(checked_div, i32, stack, crate::Trap::DivisionByZero),
        I64DivS => checked_arithmetic_method!(checked_div, i64, stack, crate::Trap::DivisionByZero),
        I32DivU => checked_arithmetic_method_cast!(checked_div, i32, u32, stack, crate::Trap::DivisionByZero),
        I64DivU => checked_arithmetic_method_cast!(checked_div, i64, u64, stack, crate::Trap::DivisionByZero),

        I32RemS => checked_arithmetic_method!(checked_wrapping_rem, i32, stack, crate::Trap::DivisionByZero),
        I64RemS => checked_arithmetic_method!(checked_wrapping_rem, i64, stack, crate::Trap::DivisionByZero),
        I32RemU => checked_arithmetic_method_cast!(checked_wrapping_rem, i32, u32, stack, crate::Trap::DivisionByZero),
        I64RemU => checked_arithmetic_method_cast!(checked_wrapping_rem, i64, u64, stack, crate::Trap::DivisionByZero),

        I32And => arithmetic_method!(bitand, i32, stack),
        I64And => arithmetic_method!(bitand, i64, stack),
        I32Or => arithmetic_method!(bitor, i32, stack),
        I64Or => arithmetic_method!(bitor, i64, stack),
        I32Xor => arithmetic_method!(bitxor, i32, stack),
        I64Xor => arithmetic_method!(bitxor, i64, stack),
        I32Shl => arithmetic_method!(wasm_shl, i32, stack),
        I64Shl => arithmetic_method!(wasm_shl, i64, stack),
        I32ShrS => arithmetic_method!(wasm_shr, i32, stack),
        I64ShrS => arithmetic_method!(wasm_shr, i64, stack),
        I32ShrU => arithmetic_method_cast!(wasm_shr, i32, u32, stack),
        I64ShrU => arithmetic_method_cast!(wasm_shr, i64, u64, stack),
        I32Rotl => arithmetic_method!(wasm_rotl, i32, stack),
        I64Rotl => arithmetic_method!(wasm_rotl, i64, stack),
        I32Rotr => arithmetic_method!(wasm_rotr, i32, stack),
        I64Rotr => arithmetic_method!(wasm_rotr, i64, stack),

        I32Clz => arithmetic_method_self!(leading_zeros, i32, stack),
        I64Clz => arithmetic_method_self!(leading_zeros, i64, stack),
        I32Ctz => arithmetic_method_self!(trailing_zeros, i32, stack),
        I64Ctz => arithmetic_method_self!(trailing_zeros, i64, stack),
        I32Popcnt => arithmetic_method_self!(count_ones, i32, stack),
        I64Popcnt => arithmetic_method_self!(count_ones, i64, stack),

        F32ConvertI32S => conv_1!(i32, f32, stack),
        F32ConvertI64S => conv_1!(i64, f32, stack),
        F64ConvertI32S => conv_1!(i32, f64, stack),
        F64ConvertI64S => conv_1!(i64, f64, stack),
        F32ConvertI32U => conv_2!(i32, u32, f32, stack),
        F32ConvertI64U => conv_2!(i64, u64, f32, stack),
        F64ConvertI32U => conv_2!(i32, u32, f64, stack),
        F64ConvertI64U => conv_2!(i64, u64, f64, stack),
        I32Extend8S => conv_2!(i32, i8, i32, stack),
        I32Extend16S => conv_2!(i32, i16, i32, stack),
        I64Extend8S => conv_2!(i64, i8, i64, stack),
        I64Extend16S => conv_2!(i64, i16, i64, stack),
        I64Extend32S => conv_2!(i64, i32, i64, stack),
        I64ExtendI32U => conv_2!(i32, u32, i64, stack),
        I64ExtendI32S => conv_1!(i32, i64, stack),
        I32WrapI64 => conv_1!(i64, i32, stack),

        F32Abs => arithmetic_method_self!(abs, f32, stack),
        F64Abs => arithmetic_method_self!(abs, f64, stack),
        F32Neg => arithmetic_method_self!(neg, f32, stack),
        F64Neg => arithmetic_method_self!(neg, f64, stack),
        F32Ceil => arithmetic_method_self!(ceil, f32, stack),
        F64Ceil => arithmetic_method_self!(ceil, f64, stack),
        F32Floor => arithmetic_method_self!(floor, f32, stack),
        F64Floor => arithmetic_method_self!(floor, f64, stack),
        F32Trunc => arithmetic_method_self!(trunc, f32, stack),
        F64Trunc => arithmetic_method_self!(trunc, f64, stack),
        F32Nearest => arithmetic_method_self!(wasm_nearest, f32, stack),
        F64Nearest => arithmetic_method_self!(wasm_nearest, f64, stack),
        F32Sqrt => arithmetic_method_self!(sqrt, f32, stack),
        F64Sqrt => arithmetic_method_self!(sqrt, f64, stack),
        F32Min => arithmetic_method!(wasm_min, f32, stack),
        F64Min => arithmetic_method!(wasm_min, f64, stack),
        F32Max => arithmetic_method!(wasm_max, f32, stack),
        F64Max => arithmetic_method!(wasm_max, f64, stack),
        F32Copysign => arithmetic_method!(copysign, f32, stack),
        F64Copysign => arithmetic_method!(copysign, f64, stack),

        // no-op instructions since types are erased at runtime
        I32ReinterpretF32 => {}
        I64ReinterpretF64 => {}
        F32ReinterpretI32 => {}
        F64ReinterpretI64 => {}

        // unsigned versions of these are a bit broken atm
        I32TruncF32S => checked_float_conv_1!(f32, i32, stack),
        I32TruncF64S => checked_float_conv_1!(f64, i32, stack),
        I32TruncF32U => checked_float_conv_2!(f32, u32, i32, stack),
        I32TruncF64U => checked_float_conv_2!(f64, u32, i32, stack),
        I64TruncF32S => checked_float_conv_1!(f32, i64, stack),
        I64TruncF64S => checked_float_conv_1!(f64, i64, stack),
        I64TruncF32U => checked_float_conv_2!(f32, u64, i64, stack),
        I64TruncF64U => checked_float_conv_2!(f64, u64, i64, stack),

        i => {
            log::error!("unimplemented instruction: {:?}", i);
            panic!("Unimplemented instruction: {:?}", i)
        }
    };

    Ok(ExecResult::Ok)
}
