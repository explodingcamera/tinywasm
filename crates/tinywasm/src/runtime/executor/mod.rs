use super::{DefaultRuntime, Stack};
use crate::{
    log::debug,
    runtime::{BlockType, LabelFrame, RawWasmValue},
    CallFrame, Error, ModuleInstance, Result, Store,
};
use alloc::vec::Vec;
use log::info;
use tinywasm_types::{BlockArgs, FuncType, Instruction, ValType};

mod macros;
use macros::*;

impl DefaultRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut Stack, module: ModuleInstance) -> Result<()> {
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
        Select => {
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
            let func = store.get_func(*v as usize)?;
            let func_ty = module.func_ty(*v);

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

        Return => todo!("called function returned"),

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
                // let params = stack.values.pop_block_params(*args, &module)?;
                cf.labels.push(LabelFrame {
                    instr_ptr: cf.instr_ptr,
                    end_instr_ptr: cf.instr_ptr + *end_offset,
                    stack_ptr: stack.values.len(), // - params,
                    args: *args,
                    ty: BlockType::If,
                });
            }
        }

        Loop(args, end_offset) => {
            // let params = stack.values.pop_block_params(*args, &module)?;
            cf.labels.push(LabelFrame {
                instr_ptr: cf.instr_ptr,
                end_instr_ptr: cf.instr_ptr + *end_offset,
                stack_ptr: stack.values.len(), // - params,
                args: *args,
                ty: BlockType::Loop,
            });
        }

        Block(args, end_offset) => {
            // let params = stack.values.pop_block_params(*args, &module)?;
            cf.labels.push(LabelFrame {
                instr_ptr: cf.instr_ptr,
                end_instr_ptr: cf.instr_ptr + *end_offset,
                stack_ptr: stack.values.len(), //- params,
                args: *args,
                ty: BlockType::Block,
            });
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

            todo!()
        }

        Br(v) => cf.break_to(*v, &mut stack.values)?,
        BrIf(v) => {
            if stack.values.pop_t::<i32>()? > 0 {
                cf.break_to(*v, &mut stack.values)?
            };
        }

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

            let res_count = match block.args {
                BlockArgs::Empty => 0,
                BlockArgs::Type(_) => 1,
                BlockArgs::FuncType(t) => module.func_ty(t).results.len(),
            };

            info!("we want to keep {} values on the stack", res_count);
            info!("current block stack ptr: {}", block.stack_ptr);
            info!("stack: {:?}", stack.values);

            // trim the lable's stack from the stack
            stack.values.truncate_keep(block.stack_ptr, res_count)
        }

        LocalGet(local_index) => stack.values.push(cf.get_local(*local_index as usize)),
        LocalSet(local_index) => cf.set_local(*local_index as usize, stack.values.pop()?),
        LocalTee(local_index) => cf.set_local(*local_index as usize, *stack.values.last()?),

        I32Const(val) => stack.values.push((*val).into()),
        I64Const(val) => stack.values.push((*val).into()),
        F32Const(val) => stack.values.push((*val).into()),
        F64Const(val) => stack.values.push((*val).into()),

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

        I64Add => arithmetic!(+, i64, stack),
        I32Add => arithmetic!(+, i32, stack),
        F32Add => arithmetic!(+, f32, stack),
        F64Add => arithmetic!(+, f64, stack),

        I32Sub => arithmetic!(-, i32, stack),
        I64Sub => arithmetic!(-, i64, stack),
        F32Sub => arithmetic!(-, f32, stack),
        F64Sub => arithmetic!(-, f64, stack),

        F32Div => arithmetic!(/, f32, stack),
        F64Div => arithmetic!(/, f64, stack),

        I32Mul => arithmetic!(*, i32, stack),
        I64Mul => arithmetic!(*, i64, stack),
        F32Mul => arithmetic!(*, f32, stack),
        F64Mul => arithmetic!(*, f64, stack),

        // these can trap
        I32DivS => checked_arithmetic!(checked_div, i32, stack, crate::Trap::DivisionByZero),
        I64DivS => checked_arithmetic!(checked_div, i64, stack, crate::Trap::DivisionByZero),
        I32DivU => checked_arithmetic_cast!(checked_div, i32, u32, stack, crate::Trap::DivisionByZero),
        I64DivU => checked_arithmetic_cast!(checked_div, i64, u64, stack, crate::Trap::DivisionByZero),

        F32ConvertI32S => conv_1!(i32, f32, stack),
        F32ConvertI64S => conv_1!(i64, f32, stack),
        F64ConvertI32S => conv_1!(i32, f64, stack),
        F64ConvertI64S => conv_1!(i64, f64, stack),
        F32ConvertI32U => conv_2!(i32, u32, f32, stack),
        F32ConvertI64U => conv_2!(i64, u64, f32, stack),
        F64ConvertI32U => conv_2!(i32, u32, f64, stack),
        F64ConvertI64U => conv_2!(i64, u64, f64, stack),
        I64ExtendI32U => conv_2!(i32, u32, i64, stack),
        I64ExtendI32S => conv_1!(i32, i64, stack),
        I32WrapI64 => conv_1!(i64, i32, stack),

        // no-op instructions since types are erased at runtime
        I32ReinterpretF32 => {}
        I64ReinterpretF64 => {}
        F32ReinterpretI32 => {}
        F64ReinterpretI64 => {}

        i => todo!("{:?}", i),
    };

    Ok(ExecResult::Ok)
}
