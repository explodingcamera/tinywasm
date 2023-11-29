use alloc::{format, string::ToString, vec, vec::Vec};
use tracing::info;
use wasmparser::Operator;

use crate::{Error, Module, Result, WasmValue};

pub fn run(module: &mut Module, func_name: &str, args: &[WasmValue]) -> Result<Vec<WasmValue>> {
    let func = module
        .exports
        .iter()
        .find(|e| e.name == func_name)
        .ok_or_else(|| Error::Other(format!("Function {} not found", func_name)))?;

    let func_type_index = module.functions[func.index as usize];
    let func_type = &module.types[func_type_index as usize];

    info!("func_type: {:#?}", func_type);
    let code = &mut module.code[func.index as usize];
    code.allow_memarg64(false);

    let mut locals = vec![];
    for ty in func_type.params() {
        locals.push(*ty);
    }

    let mut returns = vec![];
    for ty in func_type.results() {
        returns.push(*ty);
    }

    let locals_reader = code.get_locals_reader().unwrap();
    for local in locals_reader.into_iter() {
        let local = local.unwrap();
        if locals.len() != local.0 as usize {
            panic!("Invalid local index");
        }
        locals.push(local.1);
    }

    let mut local_values = vec![];
    let body = code.get_operators_reader().unwrap().into_iter();
    for (i, arg) in args.iter().enumerate() {
        if !arg.is(locals[i]) {
            return Error::other(&format!(
                "Invalid argument type for {}, index {}: expected {:?}, got {:?}",
                func_name,
                i,
                locals[i],
                arg.type_of()
            ));
        }

        local_values.push(arg);
    }

    let mut stack: Vec<WasmValue> = vec![];
    for op in body {
        let op = op.unwrap();
        info!("op: {:#?}", op);

        match op {
            Operator::LocalGet { local_index } => {
                let local = locals.get(local_index as usize).unwrap();
                let val = local_values[local_index as usize];
                info!("local: {:#?}", local);
                stack.push(val.clone());
            }
            Operator::I64Add => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                let (WasmValue::I64(a), WasmValue::I64(b)) = (a, b) else {
                    panic!("Invalid type");
                };
                let c = WasmValue::I64(a + b);
                stack.push(c);
            }
            Operator::I32Add => {
                let a = stack.pop().unwrap();
                let b = stack.pop().unwrap();
                let (WasmValue::I32(a), WasmValue::I32(b)) = (a, b) else {
                    panic!("Invalid type");
                };
                let c = WasmValue::I32(a + b);
                stack.push(c);
            }
            Operator::End => {
                info!("stack: {:#?}", stack);
                let res = returns
                    .iter()
                    .map(|ty| {
                        let val = stack.pop()?;
                        (val.is(*ty)).then_some(val)
                    })
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| Error::Other("Invalid return type".to_string()))?;

                return Ok(res);
            }
            _ => {}
        }
    }

    Error::other("End not reached")
}
