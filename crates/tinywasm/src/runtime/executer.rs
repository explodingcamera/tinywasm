use super::{Runtime, Stack};
use crate::{log::debug, runtime::RawWasmValue, Error, Result};
use alloc::vec;
use tinywasm_types::{BlockArgs, Instruction};

#[derive(Debug)]
#[allow(dead_code)]
enum BlockMarker {
    Top,
    Loop {
        instr_ptr: usize,
        stack_ptr: usize,
        args: BlockArgs,
    },
    If,
    Else,
    Block,
}

impl<const CHECK_TYPES: bool> Runtime<CHECK_TYPES> {
    pub(crate) fn exec(&self, stack: &mut Stack, instrs: &[Instruction]) -> Result<()> {
        let call_frame = stack.call_stack.top_mut()?;
        let mut instr_ptr = call_frame.instr_ptr;

        let mut blocks = vec![BlockMarker::Top];
        debug!("locals: {:?}", call_frame.locals);

        debug!("instrs: {:?}", instrs);

        // TODO: maybe we don't need to check if the instr_ptr is valid since
        // it should be validated by the parser
        while let Some(instr) = instrs.get(instr_ptr) {
            use tinywasm_types::Instruction::*;
            match instr {
                Loop(args) => {
                    blocks.push(BlockMarker::Loop {
                        instr_ptr,
                        stack_ptr: stack.values.len(),
                        args: *args,
                    });
                    debug!("loop: {:?}", args);
                    stack.values.block_args(*args)?;
                }
                BrIf(v) => {
                    // get block
                    let block = blocks
                        .get(blocks.len() - *v as usize - 1)
                        .ok_or(Error::BlockStackUnderflow)?;

                    match block {
                        BlockMarker::Loop {
                            instr_ptr: loop_instr_ptr,
                            stack_ptr: stack_size,
                            args: _,
                        } => {
                            let val = stack.values.pop().ok_or(Error::StackUnderflow)?;
                            let val: i32 = val.into();

                            // if val == 0 -> continue the loop
                            if val != 0 {
                                debug!("br_if: continue loop");
                                instr_ptr = *loop_instr_ptr;
                                stack.values.trim(*stack_size); // remove the loop values from the stack
                            }

                            // otherwise -> continue to loop end
                        }
                        _ => todo!(),
                    }
                }
                End => {
                    debug!("end, blocks: {:?}", blocks);
                    debug!("     stack: {:?}", stack.values);
                    let block = blocks.pop().ok_or(Error::BlockStackUnderflow)?;
                    match block {
                        BlockMarker::Top => {
                            debug!("end: return");
                            return Ok(());
                        }
                        BlockMarker::Loop {
                            instr_ptr: _loop_instr_ptr,
                            stack_ptr: stack_size,
                            args,
                        } => {
                            debug!("end(loop): break loop");
                            let res: &[RawWasmValue] = match args {
                                BlockArgs::Empty => &[],
                                BlockArgs::Type(_t) => todo!(),
                                BlockArgs::FuncType(_t) => todo!(),
                            };

                            stack.values.trim(stack_size); // remove the loop values from the stack
                            stack.values.extend(res.iter().copied()); // push the loop result values to the stack
                        }
                        _ => {
                            panic!("Attempted to end a block that is not the top block");
                        }
                    }
                }
                LocalGet(local_index) => {
                    let val = call_frame.get_local(*local_index as usize);
                    debug!("local: {:#?}", val);
                    stack.values.push(val);
                }
                LocalSet(local_index) => {
                    let val = stack.values.pop().ok_or(Error::StackUnderflow)?;
                    call_frame.set_local(*local_index as usize, val);
                }
                I32Const(val) => {
                    stack.values.push((*val).into());
                }
                I64Add => {
                    let [a, b] = stack.values.pop_n_const::<2>()?;
                    let a: i64 = a.into();
                    let b: i64 = b.into();
                    // let (WasmValue::I64(a), WasmValue::I64(b)) = (a, b) else {
                    //     panic!("Invalid type");
                    // };
                    let c = a + b;
                    stack.values.push(c.into());
                }
                I32Add => {
                    let [a, b] = stack.values.pop_n_const::<2>()?;
                    debug!("i64.add: {:?} + {:?}", a, b);
                    let a: i32 = a.into();
                    let b: i32 = b.into();
                    // let (WasmValue::I32(a), WasmValue::I32(b)) = (a, b) else {
                    //     panic!("Invalid type");
                    // };
                    stack.values.push((a + b).into());
                }
                I32Sub => {
                    let [a, b] = stack.values.pop_n_const::<2>()?;
                    // let (WasmValue::I32(a), WasmValue::I32(b)) = (a, b) else {
                    //     panic!("Invalid type");
                    // };
                    let a: i32 = a.into();
                    let b: i32 = b.into();
                    stack.values.push((a - b).into());
                }
                I32LtS => {
                    let [a, b] = stack.values.pop_n_const::<2>()?;
                    let a: i32 = a.into();
                    let b: i32 = b.into();
                    stack.values.push(((a < b) as i32).into());
                }
                i => todo!("{:?}", i),
            }

            instr_ptr += 1;
        }

        Err(Error::FuncDidNotReturn)
    }
}
