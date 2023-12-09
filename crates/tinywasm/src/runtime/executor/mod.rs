use super::{DefaultRuntime, Stack};
use crate::{
    log::debug,
    runtime::{BlockFrame, BlockFrameType, RawWasmValue},
    CallFrame, Error, ModuleInstance, Result, Store,
};
use alloc::vec::Vec;
use tinywasm_types::{BlockArgs, Instruction};

mod macros;
use macros::*;

impl DefaultRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut Stack, module: ModuleInstance) -> Result<()> {
        // The current call frame, gets updated inside of exec_one
        let mut cf = stack.call_stack.pop()?;

        // The function to execute, gets updated from ExecResult::Call
        let mut func = store.get_func(cf.func_ptr)?.clone();
        let mut instrs = func.instructions();

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
}

#[inline]
fn exec_one(
    cf: &mut CallFrame,
    instr: &Instruction,
    instrs: &[Instruction],
    stack: &mut Stack,
    store: &mut Store,
    module: &ModuleInstance,
) -> Result<ExecResult> {
    use tinywasm_types::Instruction::*;
    match instr {
        Nop => {} // do nothing
        Unreachable => return Err(Error::Trap(crate::Trap::Unreachable)),

        Return => {
            debug!("return");
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

        Loop(args) => {
            cf.blocks.push(BlockFrame {
                instr_ptr: cf.instr_ptr,
                stack_ptr: stack.values.len(),
                args: *args,
                ty: BlockFrameType::Loop,
            });
            stack.values.block_args(*args)?;
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
            let val: i32 = stack.values.pop().ok_or(Error::StackUnderflow)?.into();
            if val > 0 {
                cf.break_to(*v, &mut stack.values)?
            };
        }
        End => {
            let blocks = &mut cf.blocks;
            let Some(block) = blocks.pop() else {
                if stack.call_stack.is_empty() {
                    debug!("end: no block to end and no parent call frame, returning");
                    return Ok(ExecResult::Return);
                } else {
                    debug!("end: no block to end, returning to parent call frame");
                    *cf = stack.call_stack.pop()?;
                    return Ok(ExecResult::Call);
                }
            };
            debug!("end, blocks: {:?}", blocks);
            debug!("     instr_ptr: {}", cf.instr_ptr);

            match block.ty {
                BlockFrameType::Loop => {
                    debug!("end(loop): break loop");
                    let res: &[RawWasmValue] = match block.args {
                        BlockArgs::Empty => &[],
                        BlockArgs::Type(_t) => todo!(),
                        BlockArgs::FuncType(_t) => todo!(),
                    };

                    // remove the loop values from the stack
                    stack.values.trim(block.stack_ptr);

                    // push the loop result values to the stack
                    stack.values.extend(res.iter().copied());
                }
                _ => {
                    panic!("Attempted to end a block that is not the top block");
                }
            }
        }

        LocalGet(local_index) => {
            debug!("local.get: {:?}", local_index);
            let val = cf.get_local(*local_index as usize);
            debug!("local: {:#?}", val);
            stack.values.push(val);
        }
        LocalSet(local_index) => {
            debug!("local.set: {:?}", local_index);
            let val = stack.values.pop().ok_or(Error::StackUnderflow)?;
            cf.set_local(*local_index as usize, val);
        }
        LocalTee(local_index) => {
            debug!("local.tee: {:?}", local_index);
            let val = stack.values.pop().ok_or(Error::StackUnderflow)?;
            cf.set_local(*local_index as usize, val);
            stack.values.push(val);
        }
        I32Const(val) => stack.values.push((*val).into()),
        I64Const(val) => stack.values.push((*val).into()),
        I64Add => add_instr!(i64, stack),
        I32Add => add_instr!(i32, stack),
        F32Add => add_instr!(f32, stack),
        F64Add => add_instr!(f64, stack),

        I32Sub => sub_instr!(i32, stack),
        I64Sub => sub_instr!(i64, stack),
        F32Sub => sub_instr!(f32, stack),
        F64Sub => sub_instr!(f64, stack),

        I32LtS => lts_instr!(i32, stack),
        I64LtS => lts_instr!(i64, stack),
        F32Lt => lts_instr!(f32, stack),
        F64Lt => lts_instr!(f64, stack),

        I32DivS => div_instr!(i32, stack),
        I64DivS => div_instr!(i64, stack),
        F32Div => div_instr!(f32, stack),
        F64Div => div_instr!(f64, stack),

        i => todo!("{:?}", i),
    };

    Ok(ExecResult::Ok)
}
