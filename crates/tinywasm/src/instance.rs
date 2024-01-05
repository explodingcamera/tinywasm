use alloc::{boxed::Box, string::ToString, sync::Arc, vec::Vec};
use tinywasm_types::{
    DataAddr, ElmAddr, ExternalKind, FuncAddr, FuncType, GlobalAddr, Import, MemAddr, ModuleInstanceAddr, TableAddr,
};

use crate::{
    func::{FromWasmValueTuple, IntoWasmValueTuple},
    Error, ExportInstance, FuncHandle, Module, Result, Store, TypedFuncHandle,
};

/// A WebAssembly Module Instance
///
/// Addrs are indices into the store's data structures.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#module-instances>
#[derive(Debug, Clone)]
pub struct ModuleInstance(Arc<ModuleInstanceInner>);

#[derive(Debug)]
pub(crate) struct ModuleInstanceInner {
    pub(crate) store_id: usize,
    pub(crate) idx: ModuleInstanceAddr,

    pub(crate) types: Box<[FuncType]>,
    pub(crate) func_addrs: Vec<FuncAddr>,
    pub(crate) table_addrs: Vec<TableAddr>,
    pub(crate) mem_addrs: Vec<MemAddr>,
    pub(crate) global_addrs: Vec<GlobalAddr>,
    pub(crate) elem_addrs: Vec<ElmAddr>,
    pub(crate) data_addrs: Vec<DataAddr>,

    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) imports: Box<[Import]>,
    pub(crate) exports: ExportInstance,
}

impl ModuleInstance {
    /// Instantiate the module in the given store
    pub fn instantiate(store: &mut Store, module: Module) -> Result<Self> {
        let idx = store.next_module_instance_idx();

        let func_addrs = store.add_funcs(module.data.funcs.into(), idx);
        let table_addrs = store.add_tables(module.data.table_types.into(), idx);
        let mem_addrs = store.add_mems(module.data.memory_types.into(), idx);
        let global_addrs = store.add_globals(module.data.globals.into(), idx);
        let elem_addrs = store.add_elems(module.data.elements.into(), idx);
        let data_addrs = store.add_datas(module.data.data.into(), idx);

        let instance = ModuleInstanceInner {
            store_id: store.id(),
            idx,

            types: module.data.func_types,
            func_addrs,
            table_addrs,
            mem_addrs,
            global_addrs,
            elem_addrs,
            data_addrs,

            func_start: module.data.start_func,
            imports: module.data.imports,
            exports: crate::ExportInstance(module.data.exports),
        };
        let instance = ModuleInstance::new(instance);
        store.add_instance(instance.clone())?;
        Ok(instance)
    }

    /// Get the module's exports
    pub fn exports(&self) -> &ExportInstance {
        &self.0.exports
    }

    pub(crate) fn func_addrs(&self) -> &[FuncAddr] {
        &self.0.func_addrs
    }

    pub(crate) fn func_ty_addrs(&self) -> &[FuncType] {
        &self.0.types
    }

    pub(crate) fn new(inner: ModuleInstanceInner) -> Self {
        Self(Arc::new(inner))
    }

    pub(crate) fn func_ty(&self, addr: FuncAddr) -> &FuncType {
        &self.0.types[addr as usize]
    }

    // resolve a function address to the global store address
    pub(crate) fn resolve_func_addr(&self, addr: FuncAddr) -> FuncAddr {
        self.0.func_addrs[addr as usize]
    }

    /// Get an exported function by name
    pub fn exported_func_by_name(&self, store: &Store, name: &str) -> Result<FuncHandle> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }

        let export = self.0.exports.get(name, ExternalKind::Func)?;
        let func_addr = self.0.func_addrs[export.index as usize];
        let func = store.get_func(func_addr as usize)?;
        let ty = self.0.types[func.ty_addr() as usize].clone();

        Ok(FuncHandle {
            addr: export.index,
            module: self.clone(),
            name: Some(name.to_string()),
            ty,
        })
    }

    /// Get a typed exported function by name
    pub fn typed_func<P, R>(&self, store: &Store, name: &str) -> Result<TypedFuncHandle<P, R>>
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
    pub fn start_func(&self, store: &Store) -> Result<Option<FuncHandle>> {
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
    pub fn start(&self, store: &mut Store) -> Result<Option<()>> {
        let Some(func) = self.start_func(store)? else {
            return Ok(None);
        };

        let _ = func.call(store, &[])?;
        Ok(Some(()))
    }
}
