use super::{Runtime, Stack};
use crate::{Error, Result};
use log::debug;
use tinywasm_types::Instruction;

impl<const CHECK_TYPES: bool> Runtime<CHECK_TYPES> {
    pub(crate) fn exec(
        &self,
        stack: &mut Stack,
        instrs: core::slice::Iter<Instruction>,
    ) -> Result<()> {
        let call_frame = stack.call_stack.top_mut()?;

        for instr in instrs {
            use tinywasm_types::Instruction::*;
            match instr {
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
                End => {
                    debug!("stack: {:?}", stack);
                    return Ok(());
                }
                _ => todo!(),
            }
        }

        Err(Error::FuncDidNotReturn)
    }
}
