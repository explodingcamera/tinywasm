use alloc::{boxed::Box, string::ToString, sync::Arc, vec::Vec};
use tinywasm_types::{Export, ExternalKind, FuncAddr, FuncType, ModuleInstanceAddr};

use crate::{
    func::{FromWasmValueTuple, IntoWasmValueTuple},
    Error, ExportInstance, FuncHandle, Result, Store, TypedFuncHandle,
};

/// A WebAssembly Module Instance
///
/// Addrs are indices into the store's data structures.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#module-instances>
#[derive(Debug, Clone)]
pub struct ModuleInstance(Arc<ModuleInstanceInner>);

#[derive(Debug)]
struct ModuleInstanceInner {
    pub(crate) store_id: usize,
    pub(crate) _idx: ModuleInstanceAddr,
    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) types: Box<[FuncType]>,
    pub(crate) exports: ExportInstance,

    pub(crate) func_addrs: Vec<FuncAddr>,
    // pub table_addrs: Vec<TableAddr>,
    // pub mem_addrs: Vec<MemAddr>,
    // pub global_addrs: Vec<GlobalAddr>,
    // pub elem_addrs: Vec<ElmAddr>,
    // pub data_addrs: Vec<DataAddr>,
}

impl ModuleInstance {
    /// Get the module's exports
    pub fn exports(&self) -> &ExportInstance {
        &self.0.exports
    }

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

    pub(crate) fn func_ty(&self, addr: FuncAddr) -> &FuncType {
        &self.0.types[addr as usize]
    }

    // resolve a function address to the index of the function in the store
    pub(crate) fn func_addr(&self, addr: FuncAddr) -> FuncAddr {
        self.0.func_addrs[addr as usize]
    }

    pub(crate) fn func_addrs(&self) -> &[FuncAddr] {
        &self.0.func_addrs
    }

    /// Get an exported function by name
    pub fn exported_func_by_name(&self, store: &Store, name: &str) -> Result<FuncHandle> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }

        let export = self.0.exports.get(name, ExternalKind::Func)?;
        log::debug!("get_func: export: {:?}", export);

        log::debug!("{:?}", self.0.func_addrs);
        let func_addr = self.0.func_addrs[export.index as usize];
        log::debug!("get_func: func index: {}", export.index);
        let func = store.get_func(func_addr as usize)?;
        log::debug!("get_func: func_addr: {}, func: {:?}", func_addr, func);
        let ty = self.0.types[func.ty_addr() as usize].clone();
        log::debug!("get_func: ty: {:?}", ty);
        log::debug!("types: {:?}", self.0.types);

        Ok(FuncHandle {
            addr: export.index,
            module: self.clone(),
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
        let func = self.exported_func_by_name(store, name)?;
        Ok(TypedFuncHandle {
            func,
            marker: core::marker::PhantomData,
        })
    }

    /// Get the start function of the module
    ///
    /// Returns None if the module has no start function
    /// If no start function is specified, also checks for a _start function in the exports
    /// (which is not part of the spec, but used by llvm)
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#start-function>
    pub fn get_start_func(&mut self, store: &Store) -> Result<Option<FuncHandle>> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }

        let func_index = match self.0.func_start {
            Some(func_index) => func_index,
            None => {
                // alternatively, check for a _start function in the exports
                let Ok(start) = self.0.exports.get("_start", ExternalKind::Func) else {
                    return Ok(None);
                };

                start.index
            }
        };

        let func_addr = self.0.func_addrs[func_index as usize];
        let func = store.get_func(func_addr as usize)?;
        let ty = self.0.types[func.ty_addr() as usize].clone();

        Ok(Some(FuncHandle {
            module: self.clone(),
            addr: func_addr,
            ty,
            name: None,
        }))
    }

    /// Invoke the start function of the module
    ///
    /// Returns None if the module has no start function
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start>
    pub fn start(&mut self, store: &mut Store) -> Result<Option<()>> {
        let Some(func) = self.get_start_func(store)? else {
            return Ok(None);
        };

        let _ = func.call(store, &[])?;
        Ok(Some(()))
    }
}
