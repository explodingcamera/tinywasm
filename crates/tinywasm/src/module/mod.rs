use alloc::{format, vec, vec::Vec};
use wasmparser::{Export, FuncType, Validator};

use crate::{runtime::FuncAddr, Error, Result, Store, WasmValue};

use self::reader::ModuleReader;

pub mod data;
pub mod instructions;
pub mod reader;

#[derive(Debug)]
pub struct Module<'data> {
    data: ModuleReader<'data>,
}

impl<'data> Module<'data> {
    pub fn from_bytes(wasm: &'data [u8]) -> Result<Module<'data>> {
        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload?, &mut validator)?;
        }
        if !reader.end_reached {
            return Error::other("End not reached");
        }

        Ok(Self { data: reader })
    }

    /// Instantiate the module in the given store
    /// See https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation
    /// Runs the start function if it exists
    /// If you want to run the start function yourself, use `ModuleInstance::new`
    pub fn instantiate(
        &self,
        store: &'data mut Store<'data>,
        // imports: Option<()>,
    ) -> Result<ModuleInstance<'data>> {
        let i = ModuleInstance::new(store, self)?;
        let _ = i.start()?;
        Ok(i)
    }
}

/// A WebAssembly Module Instance.
/// Addrs are indices into the store's data structures.
/// See https://webassembly.github.io/spec/core/exec/runtime.html#module-instances
#[derive(Debug)]
pub struct ModuleInstance<'data> {
    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) types: Vec<FuncType>,
    // pub(crate) func_addrs: Vec<FuncAddr>,
    // pub table_addrs: Vec<TableAddr>,
    // pub mem_addrs: Vec<MemAddr>,
    // pub global_addrs: Vec<GlobalAddr>,
    // pub elem_addrs: Vec<ElmAddr>,
    // pub data_addrs: Vec<DataAddr>,
    pub(crate) exports: Vec<Export<'data>>,
}

#[derive(Debug)]
pub struct ModuleFunc {
    pub ty: FuncType,
    pub code: FuncAddr,
}

impl<'data> ModuleInstance<'data> {
    /// Get an exported function by name
    pub fn get_func(&self, name: &str) -> Option<ModuleFunc> {
        let export = self
            .exports
            .iter()
            .find(|e| e.name == name && e.kind == wasmparser::ExternalKind::Func)?;
        // let func_addr = self.func_addrs.get(export.index as usize)?;

        Some(ModuleFunc {
            code: export.index,
            ty: self.types.get(export.index as usize)?.clone(),
        })
    }

    pub fn get_start_func(&self) -> Option<ModuleFunc> {
        let func_addr = self.func_start?;

        Some(ModuleFunc {
            code: func_addr,
            ty: self.types.get(func_addr as usize)?.clone(),
        })
    }

    pub fn new(store: &'data mut Store<'data>, module: &Module<'data>) -> Result<Self> {
        let types = module
            .data
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

        // let func_addrs = module
        //     .data
        //     .function_section
        //     .as_ref()
        //     .map(|s| {
        //         s.clone()
        //             .into_iter()
        //             .map(|f| Ok(f?))
        //             .collect::<Result<Vec<_>>>()
        //     })
        //     .transpose()?
        //     .unwrap_or_default();

        let exports = module
            .data
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
        let func_start = module.data.start_func;

        store.initialize(&module.data)?;
        Ok(Self {
            types,
            func_start,
            // table_addrs,
            // mem_addrs,
            // global_addrs,
            // elem_addrs,
            // data_addrs,
            exports,
        })
    }

    pub fn call(&self, func: ModuleFunc, args: &[WasmValue]) -> Result<Vec<WasmValue>> {
        let func_type = func.ty;
        let params = func_type.params();
        if params.len() != args.len() {
            return Error::other(&format!(
                "Function expected {} arguments, got {}",
                params.len(),
                args.len()
            ));
        }

        // TODO
        // runtime.call(func, args)
        Ok(vec![])
    }

    /// Invoke the start function of the module
    /// Returns None if the module has no start function
    /// https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start
    pub fn start(&self) -> Result<Option<()>> {
        let Some(func) = self.get_start_func() else {
            return Ok(None);
        };

        let _ = self.call(func, &[])?;
        Ok(Some(()))
    }
}
