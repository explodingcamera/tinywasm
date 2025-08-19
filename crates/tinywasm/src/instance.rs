use alloc::{boxed::Box, format, rc::Rc, string::ToString};
use tinywasm_types::*;

use crate::func::{FromWasmValueTuple, IntoWasmValueTuple};
use crate::{
    Error, FuncHandle, FuncHandleTyped, Imports, MemoryRef, MemoryRefMut, Module, PotentialCoroCallResult, Result,
    Store, SuspendedFunc,
};

/// An instanciated WebAssembly module
///
/// Backed by an Rc, so cloning is cheap
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#module-instances>
#[derive(Debug, Clone)]
pub struct ModuleInstance(pub(crate) Rc<ModuleInstanceInner>);

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct ModuleInstanceInner {
    pub(crate) failed_to_instantiate: bool,

    pub(crate) store_id: usize,
    pub(crate) idx: ModuleInstanceAddr,

    pub(crate) types: Box<[FuncType]>,

    pub(crate) func_addrs: Box<[FuncAddr]>,
    pub(crate) table_addrs: Box<[TableAddr]>,
    pub(crate) mem_addrs: Box<[MemAddr]>,
    pub(crate) global_addrs: Box<[GlobalAddr]>,
    pub(crate) elem_addrs: Box<[ElemAddr]>,
    pub(crate) data_addrs: Box<[DataAddr]>,

    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) imports: Box<[Import]>,
    pub(crate) exports: Box<[Export]>,
}

impl ModuleInstance {
    // drop the module instance reference and swap it with another one
    #[inline]
    pub(crate) fn swap(&mut self, other: Self) {
        self.0 = other.0;
    }

    #[inline]
    pub(crate) fn swap_with(&mut self, other_addr: ModuleInstanceAddr, store: &mut Store) {
        if other_addr != self.id() {
            self.swap(store.get_module_instance_raw(other_addr))
        }
    }

    /// Get the module instance's address
    #[inline]
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
        let mut addrs = imports.unwrap_or_default().link(store, &module, idx)?;

        addrs.funcs.extend(store.init_funcs(module.0.funcs.into(), idx)?);
        addrs.tables.extend(store.init_tables(module.0.table_types.into(), idx)?);
        addrs.memories.extend(store.init_memories(module.0.memory_types.into(), idx)?);

        let global_addrs = store.init_globals(addrs.globals, module.0.globals.into(), &addrs.funcs, idx)?;
        let (elem_addrs, elem_trapped) =
            store.init_elements(&addrs.tables, &addrs.funcs, &global_addrs, &module.0.elements, idx)?;
        let (data_addrs, data_trapped) = store.init_datas(&addrs.memories, module.0.data.into(), idx)?;

        let instance = ModuleInstanceInner {
            failed_to_instantiate: elem_trapped.is_some() || data_trapped.is_some(),
            store_id: store.id(),
            idx,
            types: module.0.func_types,
            func_addrs: addrs.funcs.into_boxed_slice(),
            table_addrs: addrs.tables.into_boxed_slice(),
            mem_addrs: addrs.memories.into_boxed_slice(),
            global_addrs: global_addrs.into_boxed_slice(),
            elem_addrs,
            data_addrs,
            func_start: module.0.start_func,
            imports: module.0.imports,
            exports: module.0.exports,
        };

        let instance = ModuleInstance::new(instance);
        store.add_instance(instance.clone());

        match (elem_trapped, data_trapped) {
            (Some(trap), _) | (_, Some(trap)) => Err(trap.into()),
            _ => Ok(instance),
        }
    }

    /// Get a export by name
    pub fn export_addr(&self, name: &str) -> Option<ExternVal> {
        let exports = self.0.exports.iter().find(|e| e.name == name.into())?;
        let addr = match exports.kind {
            ExternalKind::Func => self.0.func_addrs.get(exports.index as usize)?,
            ExternalKind::Table => self.0.table_addrs.get(exports.index as usize)?,
            ExternalKind::Memory => self.0.mem_addrs.get(exports.index as usize)?,
            ExternalKind::Global => self.0.global_addrs.get(exports.index as usize)?,
        };

        Some(ExternVal::new(exports.kind, *addr))
    }

    #[inline]
    pub(crate) fn new(inner: ModuleInstanceInner) -> Self {
        Self(Rc::new(inner))
    }

    #[inline]
    pub(crate) fn func_ty(&self, addr: FuncAddr) -> &FuncType {
        &self.0.types[addr as usize]
    }

    #[inline]
    pub(crate) fn func_addrs(&self) -> &[FuncAddr] {
        &self.0.func_addrs
    }

    // resolve a function address to the global store address
    #[inline]
    pub(crate) fn resolve_func_addr(&self, addr: FuncAddr) -> FuncAddr {
        self.0.func_addrs[addr as usize]
    }

    // resolve a table address to the global store address
    #[inline]
    pub(crate) fn resolve_table_addr(&self, addr: TableAddr) -> TableAddr {
        self.0.table_addrs[addr as usize]
    }

    // resolve a memory address to the global store address
    #[inline]
    pub(crate) fn resolve_mem_addr(&self, addr: MemAddr) -> MemAddr {
        self.0.mem_addrs[addr as usize]
    }

    // resolve a data address to the global store address
    #[inline]
    pub(crate) fn resolve_data_addr(&self, addr: DataAddr) -> DataAddr {
        self.0.data_addrs[addr as usize]
    }

    // resolve a memory address to the global store address
    #[inline]
    pub(crate) fn resolve_elem_addr(&self, addr: ElemAddr) -> ElemAddr {
        self.0.elem_addrs[addr as usize]
    }

    // resolve a global address to the global store address
    #[inline]
    pub(crate) fn resolve_global_addr(&self, addr: GlobalAddr) -> GlobalAddr {
        self.0.global_addrs[addr as usize]
    }

    /// Get an exported function by name
    pub fn exported_func_untyped(&self, store: &Store, name: &str) -> Result<FuncHandle> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }

        let export = self.export_addr(name).ok_or_else(|| Error::Other(format!("Export not found: {name}")))?;
        let ExternVal::Func(func_addr) = export else {
            return Err(Error::Other(format!("Export is not a function: {name}")));
        };

        let ty = store.get_func(func_addr).func.ty();
        Ok(FuncHandle { addr: func_addr, module_addr: self.id(), name: Some(name.to_string()), ty: ty.clone() })
    }

    /// Get a typed exported function by name
    pub fn exported_func<P, R>(&self, store: &Store, name: &str) -> Result<FuncHandleTyped<P, R>>
    where
        P: IntoWasmValueTuple,
        R: FromWasmValueTuple,
    {
        let func = self.exported_func_untyped(store, name)?;
        Ok(FuncHandleTyped { func, marker: core::marker::PhantomData })
    }

    /// Get an exported memory by name
    pub fn exported_memory<'a>(&self, store: &'a Store, name: &str) -> Result<MemoryRef<'a>> {
        let export = self.export_addr(name).ok_or_else(|| Error::Other(format!("Export not found: {name}")))?;
        let ExternVal::Memory(mem_addr) = export else {
            return Err(Error::Other(format!("Export is not a memory: {}", name)));
        };

        self.memory(store, mem_addr)
    }

    /// Get an exported memory by name
    pub fn exported_memory_mut<'a>(&self, store: &'a mut Store, name: &str) -> Result<MemoryRefMut<'a>> {
        let export = self.export_addr(name).ok_or_else(|| Error::Other(format!("Export not found: {name}")))?;
        let ExternVal::Memory(mem_addr) = export else {
            return Err(Error::Other(format!("Export is not a memory: {}", name)));
        };

        self.memory_mut(store, mem_addr)
    }

    /// Get a memory by address
    pub fn memory<'a>(&self, store: &'a Store, addr: MemAddr) -> Result<MemoryRef<'a>> {
        let mem = store.get_mem(self.resolve_mem_addr(addr));
        Ok(MemoryRef(mem))
    }

    /// Get a memory by address (mutable)
    pub fn memory_mut<'a>(&self, store: &'a mut Store, addr: MemAddr) -> Result<MemoryRefMut<'a>> {
        let mem = store.get_mem_mut(self.resolve_mem_addr(addr));
        Ok(MemoryRefMut(mem))
    }

    /// Get the start function of the module
    ///
    /// Returns None if the module has no start function
    /// If no start function is specified, also checks for a _start function in the exports
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
                let Some(ExternVal::Func(func_addr)) = self.export_addr("_start") else {
                    return Ok(None);
                };

                func_addr
            }
        };

        let func_addr = self.resolve_func_addr(func_index);
        let func_inst = store.get_func(func_addr);
        let ty = func_inst.func.ty();

        Ok(Some(FuncHandle { module_addr: self.id(), addr: func_addr, ty: ty.clone(), name: None }))
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

    /// Invoke the start function of the module
    ///
    /// Returns None if the module has no start function
    /// If start function suspends, returns SuspededFunc.
    /// Only when it finishes can this module instance be considered instantiated
    pub fn start_coro(&self, store: &mut Store) -> Result<Option<PotentialCoroCallResult<(), SuspendedFunc>>> {
        let Some(func) = self.start_func(store)? else {
            return Ok(None);
        };

        let res = func.call_coro(store, &[])?;
        Ok(Some(res.map_result(|_| {})))
    }
}
