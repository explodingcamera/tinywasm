use alloc::{boxed::Box, string::ToString, sync::Arc, vec::Vec};
use tinywasm_types::{Export, FuncAddr, FuncType, ModuleInstanceAddr};

use crate::{
    func::{FromWasmValueTuple, IntoWasmValueTuple},
    Error, ExportInstance, FuncHandle, Result, Store, TypedFuncHandle,
};

/// A WebAssembly Module Instance
///
/// Addrs are indices into the store's data structures.
///
/// See https://webassembly.github.io/spec/core/exec/runtime.html#module-instances
#[derive(Debug, Clone)]
pub struct ModuleInstance(Arc<ModuleInstanceInner>);

#[derive(Debug)]
struct ModuleInstanceInner {
    pub(crate) store_id: usize,
    pub(crate) _idx: ModuleInstanceAddr,
    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) types: Box<[FuncType]>,
    pub exports: ExportInstance,

    pub(crate) func_addrs: Vec<FuncAddr>,
    // pub table_addrs: Vec<TableAddr>,
    // pub mem_addrs: Vec<MemAddr>,
    // pub global_addrs: Vec<GlobalAddr>,
    // pub elem_addrs: Vec<ElmAddr>,
    // pub data_addrs: Vec<DataAddr>,
}

impl ModuleInstance {
    pub(crate) fn new(
        types: Box<[FuncType]>,
        func_start: Option<FuncAddr>,
        exports: Box<[Export]>,
        func_addrs: Vec<FuncAddr>,
        idx: ModuleInstanceAddr,
        store_id: usize,
    ) -> Self {
        Self(Arc::new(ModuleInstanceInner {
            store_id,
            _idx: idx,
            types,
            func_start,
            func_addrs,
            exports: ExportInstance(exports),
        }))
    }

    /// Get an exported function by name
    pub fn get_func(&self, store: &Store, name: &str) -> Result<FuncHandle> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }

        let export = self.0.exports.func(name)?;
        let func_addr = self.0.func_addrs[export.index as usize];
        let func = store.get_func(func_addr as usize)?;
        let ty = self.0.types[func.ty_addr() as usize].clone();

        Ok(FuncHandle {
            addr: export.index,
            _module: self.clone(),
            name: Some(name.to_string()),
            ty,
        })
    }

    /// Get a typed exported function by name
    pub fn get_typed_func<P, R>(&self, store: &Store, name: &str) -> Result<TypedFuncHandle<P, R>>
    where
        P: IntoWasmValueTuple,
        R: FromWasmValueTuple,
    {
        let func = self.get_func(store, name)?;
        Ok(TypedFuncHandle {
            func,
            marker: core::marker::PhantomData,
        })
    }

    /// Get the start  function of the module
    /// Returns None if the module has no start function
    /// If no start function is specified, also checks for a _start function in the exports
    /// (which is not part of the spec, but used by llvm)
    /// https://webassembly.github.io/spec/core/syntax/modules.html#start-function
    pub fn get_start_func(&mut self, store: &Store) -> Result<Option<FuncHandle>> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }

        let func_index = match self.0.func_start {
            Some(func_index) => func_index,
            None => {
                // alternatively, check for a _start function in the exports
                let Ok(start) = self.0.exports.func("_start") else {
                    return Ok(None);
                };

                start.index
            }
        };

        let func_addr = self.0.func_addrs[func_index as usize];
        let func = store.get_func(func_addr as usize)?;
        let ty = self.0.types[func.ty_addr() as usize].clone();

        Ok(Some(FuncHandle {
            _module: self.clone(),
            addr: func_addr,
            ty,
            name: None,
        }))
    }

    /// Invoke the start function of the module
    /// Returns None if the module has no start function
    /// https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start
    pub fn start(&mut self, store: &mut Store) -> Result<Option<()>> {
        let Some(func) = self.get_start_func(store)? else {
            return Ok(None);
        };

        let _ = func.call(store, &[])?;
        Ok(Some(()))
    }
}
