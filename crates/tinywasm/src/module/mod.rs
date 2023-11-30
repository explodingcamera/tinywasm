use alloc::vec::Vec;
use wasmparser::{Export, FuncType, Validator};

use crate::{engine::FuncAddr, Error, Result, Store};

use self::reader::ModuleReader;

pub mod reader;

#[derive(Debug)]
pub struct Module<'a> {
    store: &'a mut Store<'a>,
    reader: ModuleReader<'a>,
}

/// A WebAssembly Module Instance.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#module-instances
#[derive(Debug)]
pub struct ModuleInstance<'m, 'data> {
    pub module: &'m Module<'data>,

    pub types: Vec<FuncType>,
    pub func_addrs: Vec<FuncAddr>,
    // pub table_addrs: Vec<TableAddr>,
    // pub mem_addrs: Vec<MemAddr>,
    // pub global_addrs: Vec<GlobalAddr>,
    // pub elem_addrs: Vec<ElmAddr>,
    // pub data_addrs: Vec<DataAddr>,
    pub exports: Vec<Export<'data>>,
}

impl<'m, 'data> ModuleInstance<'m, 'data> {
    pub fn new(module: &'m mut Module<'data>) -> Result<Self> {
        let types = module
            .reader
            .type_section
            .take()
            .map(|s| {
                s.into_iter()
                    .map(|ty| {
                        let wasmparser::Type::Func(func) = ty?;
                        Ok(func)
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        let func_addrs = module
            .reader
            .function_section
            .take()
            .map(|s| s.into_iter().map(|f| Ok(f?)).collect::<Result<Vec<_>>>())
            .transpose()?
            .unwrap_or_default();

        let exports = module
            .reader
            .export_section
            .take()
            .map(|s| s.into_iter().map(|e| Ok(e?)).collect::<Result<Vec<_>>>())
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            module,

            types,
            func_addrs,
            // table_addrs,
            // mem_addrs,
            // global_addrs,
            // elem_addrs,
            // data_addrs,
            exports,
        })
    }
}

impl<'a> Module<'a> {
    pub fn try_new(store: &'a mut Store<'a>, wasm: &'a [u8]) -> Result<Module<'a>> {
        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
            reader.process_payload(payload?, &mut validator)?;
        }
        if !reader.end_reached {
            return Error::other("End not reached");
        }

        Ok(Self { store, reader })
    }
}
