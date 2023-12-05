mod executer;
mod stack;

use log::debug;
pub use stack::*;
use tinywasm_types::{Instruction, WasmValue};

use crate::{Error, Result};

/// A WebAssembly Runtime.
/// See https://webassembly.github.io/spec/core/exec/runtime.html
///
/// Generic over `CheckTypes` to enable type checking at runtime.
/// This is useful for debugging, but should be disabled if you know
/// that the module is valid.
#[derive(Debug, Default)]
pub struct Runtime<const CHECK_TYPES: bool> {}

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

                    let (WasmValue::I64(a), WasmValue::I64(b)) = (a, b) else {
                        panic!("Invalid type");
                    };
                    let c = WasmValue::I64(a + b);
                    stack.values.push(c);
                }
                I32Add => {
                    let [a, b] = stack.values.pop_n_const::<2>()?;
                    debug!("i64.add: {:?} + {:?}", a, b);

                    let (WasmValue::I32(a), WasmValue::I32(b)) = (a, b) else {
                        panic!("Invalid type");
                    };
                    let c = WasmValue::I32(a + b);
                    debug!("i64.add: {:?}", c);
                    stack.values.push(c);
                }
                I32Sub => {
                    let [a, b] = stack.values.pop_n_const::<2>()?;
                    let (WasmValue::I32(a), WasmValue::I32(b)) = (a, b) else {
                        panic!("Invalid type");
                    };
                    let c = WasmValue::I32(a - b);
                    stack.values.push(c);
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
