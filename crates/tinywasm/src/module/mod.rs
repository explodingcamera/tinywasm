use core::fmt::Debug;

use crate::error::{Error, Result};
use alloc::vec::Vec;
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
