use alloc::{boxed::Box, format, string::ToString, sync::Arc, vec::Vec};
use tinywasm_types::{
    DataAddr, ElemAddr, Export, ExternVal, ExternalKind, FuncAddr, FuncType, GlobalAddr, Import, MemAddr,
    ModuleInstanceAddr, TableAddr,
};

use crate::{
    func::{FromWasmValueTuple, IntoWasmValueTuple},
    Error, FuncHandle, Imports, Module, Result, Store, TypedFuncHandle,
};

/// A WebAssembly Module Instance
///
/// Addrs are indices into the store's data structures.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#module-instances>
#[derive(Debug, Clone)]
pub struct ModuleInstance(Arc<ModuleInstanceInner>);

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct ModuleInstanceInner {
    pub(crate) store_id: usize,
    pub(crate) idx: ModuleInstanceAddr,

    pub(crate) types: Box<[FuncType]>,
    pub(crate) func_addrs: Vec<FuncAddr>,
    pub(crate) table_addrs: Vec<TableAddr>,
    pub(crate) mem_addrs: Vec<MemAddr>,
    pub(crate) global_addrs: Vec<GlobalAddr>,
    pub(crate) elem_addrs: Vec<ElemAddr>,
    pub(crate) data_addrs: Vec<DataAddr>,

    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) imports: Box<[Import]>,
    pub(crate) exports: Box<[Export]>,
}

impl ModuleInstance {
    /// Get the module instance's address
    pub fn id(&self) -> ModuleInstanceAddr {
        self.0.idx
    }

    /// Instantiate the module in the given store
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation>
    pub fn instantiate(store: &mut Store, module: Module, imports: Option<Imports>) -> Result<Self> {
        // This doesn't completely follow the steps in the spec, but the end result is the same
        // Constant expressions are evaluated directly where they are used, so we
        // don't need to create a auxiliary frame etc.

        let idx = store.next_module_instance_idx();
        let imports = imports.unwrap_or_default();

        let mut addrs = imports.link(store, &module, idx)?;
        let data = module.data;

        addrs.globals.extend(store.init_globals(data.globals.into(), idx)?);
        addrs.funcs.extend(store.init_funcs(data.funcs.into(), idx)?);
        addrs.tables.extend(store.init_tables(data.table_types.into(), idx)?);

        log::info!("init_mems: {:?}", addrs.mems);
        addrs.mems.extend(store.init_mems(data.memory_types.into(), idx)?);
        log::info!("init_mems2: {:?}", addrs.mems);
        log::info!("init_mems g: {:?}", store.data.mems.len());

        let elem_addrs = store.init_elems(&addrs.tables, data.elements.into(), idx)?;
        log::info!("init_elems: {:?}", addrs.mems);

        let data_addrs = store.init_datas(&addrs.mems, data.data.into(), idx)?;
        log::info!("init_datas: {:?}", addrs.mems);

        let instance = ModuleInstanceInner {
            store_id: store.id(),
            idx,

            types: data.func_types,

            func_addrs: addrs.funcs,
            table_addrs: addrs.tables,
            mem_addrs: addrs.mems,
            global_addrs: addrs.globals,
            elem_addrs,
            data_addrs,

            func_start: data.start_func,
            imports: data.imports,
            exports: data.exports,
        };

        let instance = ModuleInstance::new(instance);
        store.add_instance(instance.clone())?;

        Ok(instance)
    }

    /// Get the module's exports
    pub(crate) fn export(&self, name: &str) -> Option<ExternVal> {
        let exports = self.0.exports.iter().find(|e| e.name == name.into())?;
        let kind = exports.kind.clone();
        let addr = match kind {
            ExternalKind::Func => self.0.func_addrs.get(exports.index as usize)?,
            ExternalKind::Table => self.0.table_addrs.get(exports.index as usize)?,
            ExternalKind::Memory => self.0.mem_addrs.get(exports.index as usize)?,
            ExternalKind::Global => self.0.global_addrs.get(exports.index as usize)?,
        };

        Some(ExternVal::new(kind, *addr))
    }

    pub(crate) fn func_addrs(&self) -> &[FuncAddr] {
        &self.0.func_addrs
    }

    pub(crate) fn _global_addrs(&self) -> &[GlobalAddr] {
        &self.0.global_addrs
    }

    pub(crate) fn func_ty_addrs(&self) -> &[FuncType] {
        &self.0.types
    }

    pub(crate) fn new(inner: ModuleInstanceInner) -> Self {
        Self(Arc::new(inner))
    }

    pub(crate) fn func_ty(&self, addr: FuncAddr) -> &FuncType {
        &self.0.types.get(addr as usize).expect("No func type for func, this is a bug")
    }

    // resolve a function address to the global store address
    pub(crate) fn resolve_func_addr(&self, addr: FuncAddr) -> FuncAddr {
        *self.0.func_addrs.get(addr as usize).expect("No func addr for func, this is a bug")
    }

    // resolve a table address to the global store address
    pub(crate) fn resolve_table_addr(&self, addr: TableAddr) -> TableAddr {
        *self.0.table_addrs.get(addr as usize).expect("No table addr for table, this is a bug")
    }

    // resolve a memory address to the global store address
    pub(crate) fn resolve_mem_addr(&self, addr: MemAddr) -> MemAddr {
        *self.0.mem_addrs.get(addr as usize).expect("No mem addr for mem, this is a bug")
    }

    // resolve a global address to the global store address
    pub(crate) fn resolve_global_addr(&self, addr: GlobalAddr) -> GlobalAddr {
        self.0.global_addrs[addr as usize]
    }

    /// Get an exported function by name
    pub fn exported_func_by_name(&self, store: &Store, name: &str) -> Result<FuncHandle> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }

        let export = self.export(name).ok_or_else(|| Error::Other(format!("Export not found: {}", name)))?;
        let ExternVal::Func(func_addr) = export else {
            return Err(Error::Other(format!("Export is not a function: {}", name)));
        };

        let func_inst = store.get_func(func_addr as usize)?;
        let ty = func_inst.func.ty(&self);

        Ok(FuncHandle { addr: func_addr, module: self.clone(), name: Some(name.to_string()), ty: ty.clone() })
    }

    /// Get a typed exported function by name
    pub fn typed_func<P, R>(&self, store: &Store, name: &str) -> Result<TypedFuncHandle<P, R>>
    where
        P: IntoWasmValueTuple,
        R: FromWasmValueTuple,
    {
        let func = self.exported_func_by_name(store, name)?;
        Ok(TypedFuncHandle { func, marker: core::marker::PhantomData })
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
                let Some(ExternVal::Func(func_addr)) = self.export("_start") else {
                    return Ok(None);
                };

                func_addr
            }
        };

        let func_addr = self.0.func_addrs.get(func_index as usize).expect("No func addr for start func, this is a bug");

        let func_inst = store.get_func(*func_addr as usize)?;
        let ty = func_inst.func.ty(&self);

        Ok(Some(FuncHandle { module: self.clone(), addr: *func_addr, ty, name: None }))
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
