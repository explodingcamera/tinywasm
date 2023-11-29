use core::fmt::Debug;

use crate::error::{Error, Result};
use alloc::{format, string::String, vec, vec::Vec};
use hashbrown::{HashMap, HashSet};
use tracing::{error, info};
use wasmparser::*;

mod reader;
use self::reader::ModuleReader;

#[derive(Debug)]
pub struct ModuleMetadata {
    pub version: u16,
}

pub struct Module<'data> {
    pub meta: ModuleMetadata,

    pub types: Vec<FuncType>,
    pub functions: Vec<u32>,
    pub exports: Vec<Export<'data>>,
    pub code: Vec<FunctionBody<'data>>,

    marker: core::marker::PhantomData<&'data ()>,
}

impl Debug for Module<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Module")
            .field("meta", &self.meta)
            .field("types", &self.types)
            .field("functions", &self.functions)
            .field("exports", &self.exports)
            .field("code", &self.code)
            .finish()
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum WasmValue {
    I32(i32),
    I64(i64),
}

impl WasmValue {
    fn to_bytes(&self) -> (Vec<u8>, ValType) {
        match self {
            Self::I32(val) => {
                let val = val.to_le_bytes().to_vec();
                (val, ValType::I32)
            }
            Self::I64(val) => {
                let val = val.to_le_bytes().to_vec();
                (val, ValType::I64)
            } // _ => {
              //     panic!("Unsupported return type");
              // }
        }
    }

    fn from_bytes(bytes: &[u8], ty: &ValType) -> Self {
        match ty {
            ValType::I32 => {
                let val = i32::from_le_bytes(bytes.try_into().unwrap());
                Self::I32(val)
            }
            ValType::I64 => {
                let val = i64::from_le_bytes(bytes.try_into().unwrap());
                Self::I64(val)
            }
            _ => {
                panic!("Unsupported return type");
            }
        }
    }
}

impl<'data> Module<'data> {
    pub fn new(wasm: &'data [u8]) -> Result<Self> {
        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload?, &mut validator)?;
        }

        if !reader.end_reached {
            return Error::other("End not reached");
        }

        Self::from_reader(reader)
    }

    pub fn run(&mut self, func_name: &str, args: &[WasmValue]) -> Result<Vec<WasmValue>> {
        let func = self
            .exports
            .iter()
            .find(|e| e.name == func_name)
            .ok_or_else(|| Error::Other(format!("Function {} not found", func_name)))?;

        let func_type_index = self.functions[func.index as usize];
        let func_type = &self.types[func_type_index as usize];

        info!("func_type: {:#?}", func_type);
        let code = &mut self.code[func.index as usize];
        code.allow_memarg64(false);

        let mut locals = vec![];
        for ty in func_type.params() {
            locals.push(ty.clone());
        }

        let mut returns = vec![];
        for ty in func_type.results() {
            returns.push(ty.clone());
        }

        let locals_reader = code.get_locals_reader().unwrap();
        for local in locals_reader.into_iter() {
            let local = local.unwrap();
            if locals.len() != local.0 as usize {
                panic!("Invalid local index");
            }
            locals.push(local.1);
        }

        let mut body = code.get_operators_reader().unwrap().into_iter();

        let mut local_values = vec![];
        for (i, arg) in args.iter().enumerate() {
            let (val, ty) = arg.to_bytes();
            if locals[i] != ty {
                return Error::other(&format!(
                    "Invalid argument type for {}, index {}: expected {:?}, got {:?}",
                    func_name, i, locals[i], ty
                ));
            }

            local_values.push(val);
        }

        let mut stack: Vec<Vec<u8>> = Vec::new();
        while let Some(op) = body.next() {
            let op = op.unwrap();
            info!("op: {:#?}", op);

            match op {
                Operator::LocalGet { local_index } => {
                    let local = locals.get(local_index as usize).unwrap();
                    let val = &local_values[local_index as usize];
                    info!("local: {:#?}", local);
                    stack.push(val.clone());
                }
                Operator::I64Add => {
                    let a = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    let a = i64::from_le_bytes(a.try_into().unwrap());
                    let b = i64::from_le_bytes(b.try_into().unwrap());
                    let c = (a + b).to_le_bytes().to_vec();
                    stack.push(c);
                }
                Operator::I32Add => {
                    let a = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    let a = i32::from_le_bytes(a.try_into().unwrap());
                    let b = i32::from_le_bytes(b.try_into().unwrap());
                    let c = (a + b).to_le_bytes().to_vec();
                    stack.push(c);
                }
                Operator::End => {
                    info!("stack: {:#?}", stack);
                    let res = returns
                        .iter()
                        .map(|ty| {
                            let val = stack.pop().unwrap();
                            WasmValue::from_bytes(&val, ty)
                        })
                        .collect::<Vec<_>>();
                    return Ok(res);
                }
                _ => {}
            }
        }

        return Error::other("End not reached");
    }

    fn from_reader(reader: ModuleReader<'data>) -> Result<Self> {
        let types = reader
            .type_section
            .map(|s| {
                s.into_iter()
                    .map(|ty| {
                        let Type::Func(func) = ty?;
                        Ok(func)
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        let functions = reader
            .function_section
            .map(|s| s.into_iter().map(|f| Ok(f?)).collect::<Result<Vec<_>>>())
            .transpose()?
            .unwrap_or_default();

        let exports = reader
            .export_section
            .map(|s| s.into_iter().map(|e| Ok(e?)).collect::<Result<Vec<_>>>())
            .transpose()?
            .unwrap_or_default();

        let code = reader.code_section.map(|s| s.functions).unwrap_or_default();

        let meta = ModuleMetadata {
            version: reader.version.unwrap_or(1),
        };

        Ok(Self {
            marker: core::marker::PhantomData,
            meta,
            types,
            exports,
            functions,
            code,
        })
    }
}
