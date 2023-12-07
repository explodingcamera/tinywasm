use super::{Runtime, Stack};
use crate::{
    log::debug,
    runtime::{BlockFrame, BlockFrameType, RawWasmValue},
    Error, Result,
};
use alloc::vec::Vec;
use tinywasm_types::{BlockArgs, Instruction};

mod macros;
use macros::*;

impl<const CHECK_TYPES: bool> Runtime<CHECK_TYPES> {
    pub(crate) fn exec(&self, stack: &mut Stack, instrs: &[Instruction]) -> Result<()> {
        let cf = stack.call_stack.top_mut()?;

        // TODO: maybe we don't need to check if the instr_ptr is valid since
        // it should be validated by the parser
        while let Some(instr) = instrs.get(cf.instr_ptr) {
            use tinywasm_types::Instruction::*;
            match instr {
                Nop => {} // do nothing
                Unreachable => return Err(Error::Trap(crate::Trap::Unreachable)),
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
                        debug!("end: no block to end, returning");
                        return Ok(());
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
                I32Const(val) => {
                    stack.values.push((*val).into());
                }
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
            }

            cf.instr_ptr += 1;
        }

        debug!("end of exec");
        debug!("stack: {:?}", stack.values);
        debug!("insts: {:?}", instrs);
        debug!("instr_ptr: {}", cf.instr_ptr);
        Err(Error::FuncDidNotReturn)
    }
}
