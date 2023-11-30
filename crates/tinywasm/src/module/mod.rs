use alloc::vec::Vec;
use wasmparser::{Export, FuncType, Validator};

use crate::{
    runtime::{FuncAddr, ModuleFunc},
    Error, Result, Store,
};

use self::reader::ModuleReader;

pub mod reader;

#[derive(Debug)]
pub struct Module<'a> {
    reader: ModuleReader<'a>,
}

/// A WebAssembly Module Instance.
/// Addrs are indices into the store's data structures.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#module-instances
#[derive(Debug)]
pub struct ModuleInstance<'m, 'data> {
    pub(crate) module: &'m Module<'data>,

    pub(crate) types: Vec<FuncType>,
    pub(crate) func_addrs: Vec<FuncAddr>,
    // pub table_addrs: Vec<TableAddr>,
    // pub mem_addrs: Vec<MemAddr>,
    // pub global_addrs: Vec<GlobalAddr>,
    // pub elem_addrs: Vec<ElmAddr>,
    // pub data_addrs: Vec<DataAddr>,
    pub(crate) exports: Vec<Export<'data>>,
}

impl<'m, 'data> ModuleInstance<'m, 'data>
where
    'm: 'data,
{
    /// Get an exported function by name
    pub fn get_func(&self, name: &str) -> Option<ModuleFunc> {
        let export = self
            .exports
            .iter()
            .find(|e| e.name == name && e.kind == wasmparser::ExternalKind::Func)?;
        let func_addr = self.func_addrs.get(export.index as usize)?;

        Some(ModuleFunc {
            code: *func_addr,
            ty: self.types.get(*func_addr as usize)?.clone(),
        })
    }

    pub fn new(store: &'data mut Store<'data>, module: &'m Module<'data>) -> Result<Self> {
        let types = module
            .reader
            .type_section
            .as_ref()
            .map(|s| {
                s.clone()
                    .into_iter()
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
            .as_ref()
            .map(|s| {
                s.clone()
                    .into_iter()
                    .map(|f| Ok(f?))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        let exports = module
            .reader
            .export_section
            .as_ref()
            .map(|s| {
                s.clone()
                    .into_iter()
                    .map(|e| Ok(e?))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        store.initialize(&module.reader)?;
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
    pub fn try_new(wasm: &'a [u8]) -> Result<Module<'a>> {
        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload?, &mut validator)?;
        }
        if !reader.end_reached {
            return Error::other("End not reached");
        }

        Ok(Self { reader })
    }
}
