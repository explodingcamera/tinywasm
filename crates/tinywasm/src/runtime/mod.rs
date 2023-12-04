mod executer;
mod stack;

use log::debug;
pub use stack::*;
use tinywasm_types::{Instruction, WasmValue};

use crate::{Error, Result};

/// A WebAssembly Runtime.
/// See https://webassembly.github.io/spec/core/exec/runtime.html
#[derive(Debug, Default)]
pub struct Runtime {}

impl Runtime {
    pub(crate) fn exec(
        &self,
        stack: &mut Stack,
        instrs: core::slice::Iter<Instruction>,
    ) -> Result<()> {
        let locals = &mut stack.locals;
        let value_stack = &mut stack.value_stack;

        for instr in instrs {
            use tinywasm_types::Instruction::*;
            match instr {
                LocalGet(local_index) => {
                    let val = &locals[*local_index as usize];
                    debug!("local: {:#?}", val);
                    value_stack.push(val.clone());
                }
                I64Add => {
                    let a = value_stack.pop().unwrap();
                    let b = value_stack.pop().unwrap();
                    let (WasmValue::I64(a), WasmValue::I64(b)) = (a, b) else {
                        panic!("Invalid type");
                    };
                    let c = WasmValue::I64(a + b);
                    value_stack.push(c);
                }
                I32Add => {
                    let a = value_stack.pop().unwrap();
                    let b = value_stack.pop().unwrap();
                    let (WasmValue::I32(a), WasmValue::I32(b)) = (a, b) else {
                        panic!("Invalid type");
                    };
                    let c = WasmValue::I32(a + b);
                    value_stack.push(c);
                }
                End => {
                    return Ok(());
                }
                _ => todo!(),
            }
        }

        Err(Error::FuncDidNotReturn)
    }
}
