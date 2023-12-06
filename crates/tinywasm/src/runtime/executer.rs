use super::{Runtime, Stack};
use crate::{log::debug, Error, Result};
use alloc::vec;
use tinywasm_types::Instruction;

#[allow(dead_code)]
enum BlockMarker {
    Top,
    Loop,
    If,
    Else,
    Block,
}

impl<const CHECK_TYPES: bool> Runtime<CHECK_TYPES> {
    pub(crate) fn exec(&self, stack: &mut Stack, instrs: core::slice::Iter<Instruction>) -> Result<()> {
        let call_frame = stack.call_stack.top_mut()?;
        let mut blocks = vec![BlockMarker::Top];

        for instr in instrs {
            use tinywasm_types::Instruction::*;
            match instr {
                End => {
                    let block = blocks.pop().ok_or(Error::BlockStackUnderflow)?;

                    use BlockMarker::*;
                    match block {
                        Top => return Ok(()),
                        Block => todo!(),
                        Loop => todo!(),
                        If => todo!(),
                        Else => todo!(),
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
                _ => todo!(),
            }
        }

        Err(Error::FuncDidNotReturn)
    }
}
