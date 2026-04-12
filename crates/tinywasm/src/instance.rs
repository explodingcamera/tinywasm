use alloc::boxed::Box;
use alloc::{format, rc::Rc};
use tinywasm_types::*;

use crate::func::{FromWasmValueTuple, IntoWasmValueTuple, ValTypesFromTuple};
use crate::{
    Error, FuncHandle, FuncHandleTyped, GlobalRef, GlobalRefMut, Imports, MemoryRef, MemoryRefMut, Module, Result,
    Store, TableRef, TableRefMut,
};

/// A typed borrowed view over an exported extern value.
pub enum ExternItemRef<'a> {
    /// Exported function handle.
    Func(FuncHandle),
    /// Exported memory reference.
    Memory(MemoryRef<'a>),
    /// Exported table reference.
    Table(TableRef<'a>),
    /// Exported global reference.
    Global(GlobalRef<'a>),
}

/// A typed mutable borrowed view over an exported extern value.
pub enum ExternItemRefMut<'a> {
    /// Exported function handle.
    Func(FuncHandle),
    /// Exported mutable memory reference.
    Memory(MemoryRefMut<'a>),
    /// Exported mutable table reference.
    Table(TableRefMut<'a>),
    /// Exported mutable global reference.
    Global(GlobalRefMut<'a>),
}

/// An instantiated WebAssembly module
///
/// Backed by an Rc, so cloning is cheap
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#module-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct ModuleInstance(pub(crate) Rc<ModuleInstanceInner>);

#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct ModuleInstanceInner {
    pub(crate) store_id: usize,
    pub(crate) idx: ModuleInstanceAddr,
    pub(crate) types: ArcSlice<FuncType>,
    pub(crate) func_addrs: Box<[FuncAddr]>,
    pub(crate) table_addrs: Box<[TableAddr]>,
    pub(crate) mem_addrs: Box<[MemAddr]>,
    pub(crate) global_addrs: Box<[GlobalAddr]>,
    pub(crate) elem_addrs: Box<[ElemAddr]>,
    pub(crate) data_addrs: Box<[DataAddr]>,
    pub(crate) func_start: Option<FuncAddr>,
    pub(crate) exports: ArcSlice<Export>,
}

impl ModuleInstanceInner {
    #[inline]
    pub(crate) fn func_ty(&self, addr: FuncAddr) -> &FuncType {
        match self.types.get(addr as usize) {
            Some(ty) => ty,
            None => unreachable!("invalid function address: {addr}"),
        }
    }

    #[inline]
    pub(crate) fn func_addrs(&self) -> &[FuncAddr] {
        &self.func_addrs
    }

    // resolve a function address to the global store address
    #[inline]
    pub(crate) fn resolve_func_addr(&self, addr: FuncAddr) -> FuncAddr {
        match self.func_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => unreachable!("invalid function address: {addr}"),
        }
    }

    // resolve a table address to the global store address
    #[inline]
    pub(crate) fn resolve_table_addr(&self, addr: TableAddr) -> TableAddr {
        match self.table_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => unreachable!("invalid table address: {addr}"),
        }
    }

    // resolve a memory address to the global store address
    #[inline]
    pub(crate) fn resolve_mem_addr(&self, addr: MemAddr) -> MemAddr {
        match self.mem_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => unreachable!("invalid memory address: {addr}"),
        }
    }

    // resolve a data address to the global store address
    #[inline]
    pub(crate) fn resolve_data_addr(&self, addr: DataAddr) -> DataAddr {
        match self.data_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => unreachable!("invalid data address: {addr}"),
        }
    }

    // resolve a memory address to the global store address
    #[inline]
    pub(crate) fn resolve_elem_addr(&self, addr: ElemAddr) -> ElemAddr {
        match self.elem_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => unreachable!("invalid element address: {addr}"),
        }
    }

    // resolve a global address to the global store address
    #[inline]
    pub(crate) fn resolve_global_addr(&self, addr: GlobalAddr) -> GlobalAddr {
        match self.global_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => unreachable!("invalid global address: {addr}"),
        }
    }
}

impl ModuleInstance {
    #[inline]
    fn validate_store(&self, store: &Store) -> Result<()> {
        if self.0.store_id != store.id() {
            return Err(Error::InvalidStore);
        }
        Ok(())
    }

    /// Get the module instance's address
    pub fn id(&self) -> ModuleInstanceAddr {
        self.0.idx
    }

    /// Instantiate the module in the given store
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation>
    pub fn instantiate(store: &mut Store, module: Module, imports: Option<Imports>) -> Result<Self> {
        let idx = store.next_module_instance_idx();
        let mut addrs = imports.unwrap_or_default().link(store, &module, idx)?;

        addrs.funcs.extend(store.init_funcs(&module.0.funcs, idx)?);
        addrs.tables.extend(store.init_tables(&module.0.table_types, idx)?);
        addrs.memories.extend(store.init_memories(&module.0.memory_types, idx)?);
        let global_addrs = store.init_globals(addrs.globals, &module.0.globals, &addrs.funcs, idx)?;
        let (elem_addrs, elem_trapped) =
            store.init_elements(&addrs.tables, &addrs.funcs, &global_addrs, &module.0.elements, idx)?;
        let (data_addrs, data_trapped) = store.init_data(&addrs.memories, &module.0.data, idx)?;

        let instance = ModuleInstanceInner {
            store_id: store.id(),
            idx,
            types: module.0.func_types.clone(),
            func_addrs: addrs.funcs.into_boxed_slice(),
            table_addrs: addrs.tables.into_boxed_slice(),
            mem_addrs: addrs.memories.into_boxed_slice(),
            global_addrs: global_addrs.into_boxed_slice(),
            elem_addrs,
            data_addrs,
            func_start: module.0.start_func,
            exports: module.0.exports.clone(),
        };

        let instance = Rc::new(instance);
        store.add_instance(instance.clone());

        match (elem_trapped, data_trapped) {
            (Some(trap), _) | (_, Some(trap)) => Err(trap.into()),
            _ => Ok(ModuleInstance(instance)),
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

    /// Returns an iterator over all exported extern values for this instance.
    pub fn exports<'a>(&'a self, store: &'a Store) -> Result<impl Iterator<Item = (&'a str, ExternItemRef<'a>)> + 'a> {
        self.validate_store(store)?;

        Ok(self.0.exports.iter().map(move |export| {
            let name = export.name.as_ref();
            let item = match export.kind {
                ExternalKind::Func => {
                    let idx = export.index as usize;
                    let func_addr = *self
                        .0
                        .func_addrs
                        .get(idx)
                        .unwrap_or_else(|| unreachable!("invalid function export index: {}", export.index));
                    let ty = store.state.get_func(func_addr).func.ty();
                    ExternItemRef::Func(FuncHandle {
                        store_id: self.0.store_id,
                        module_addr: self.id(),
                        addr: func_addr,
                        ty: ty.clone(),
                    })
                }
                ExternalKind::Table => {
                    let idx = export.index as usize;
                    let table_addr = *self
                        .0
                        .table_addrs
                        .get(idx)
                        .unwrap_or_else(|| unreachable!("invalid table export index: {}", export.index));
                    ExternItemRef::Table(TableRef(store.state.get_table(table_addr)))
                }
                ExternalKind::Memory => {
                    let idx = export.index as usize;
                    let mem_addr = *self
                        .0
                        .mem_addrs
                        .get(idx)
                        .unwrap_or_else(|| unreachable!("invalid memory export index: {}", export.index));
                    ExternItemRef::Memory(MemoryRef(store.state.get_mem(mem_addr)))
                }
                ExternalKind::Global => {
                    let idx = export.index as usize;
                    let global_addr = *self
                        .0
                        .global_addrs
                        .get(idx)
                        .unwrap_or_else(|| unreachable!("invalid global export index: {}", export.index));
                    ExternItemRef::Global(GlobalRef(store.state.get_global(global_addr)))
                }
            };

            (name, item)
        }))
    }

    #[inline]
    fn require_export(&self, name: &str) -> Result<ExternVal> {
        self.export_addr(name).ok_or_else(|| Error::Other(format!("Export not found: {name}")))
    }

    #[inline]
    #[cfg(feature = "guest_debug")]
    fn index_addr<T: Copy>(slice: &[T], idx: u32, kind: &str) -> Result<T> {
        slice.get(idx as usize).copied().ok_or_else(|| Error::Other(format!("{kind} index out of bounds: {idx}")))
    }

    /// Get any exported extern value by name.
    pub fn extern_item<'a>(&self, store: &'a Store, name: &str) -> Result<ExternItemRef<'a>> {
        self.validate_store(store)?;
        match self.require_export(name)? {
            ExternVal::Func(_) => self.func(store, name).map(ExternItemRef::Func),
            ExternVal::Memory(mem_addr) => Ok(ExternItemRef::Memory(MemoryRef(store.state.get_mem(mem_addr)))),
            ExternVal::Table(table_addr) => Ok(ExternItemRef::Table(TableRef(store.state.get_table(table_addr)))),
            ExternVal::Global(global_addr) => Ok(ExternItemRef::Global(GlobalRef(store.state.get_global(global_addr)))),
        }
    }

    /// Get any exported extern value by name with mutable access when applicable.
    pub fn extern_item_mut<'a>(&self, store: &'a mut Store, name: &str) -> Result<ExternItemRefMut<'a>> {
        self.validate_store(store)?;
        match self.require_export(name)? {
            ExternVal::Func(_) => self.func(store, name).map(ExternItemRefMut::Func),
            ExternVal::Memory(mem_addr) => {
                Ok(ExternItemRefMut::Memory(MemoryRefMut(store.state.get_mem_mut(mem_addr))))
            }
            ExternVal::Table(table_addr) => {
                Ok(ExternItemRefMut::Table(TableRefMut(store.state.get_table_mut(table_addr))))
            }
            ExternVal::Global(global_addr) => {
                Ok(ExternItemRefMut::Global(GlobalRefMut(store.state.get_global_mut(global_addr))))
            }
        }
    }

    /// Get a function export by name.
    pub fn func(&self, store: &Store, name: &str) -> Result<FuncHandle> {
        self.validate_store(store)?;

        let export = self.require_export(name)?;
        let ExternVal::Func(func_addr) = export else {
            return Err(Error::Other(format!("Export is not a function: {name}")));
        };

        let ty = store.state.get_func(func_addr).func.ty();
        Ok(FuncHandle { store_id: self.0.store_id, addr: func_addr, module_addr: self.id(), ty: ty.clone() })
    }

    /// Get a function by its module-local index.
    ///
    /// This exposes an internal module-owned function directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// introspection. Calling private functions can change behavior in ways the
    /// module author did not expose as part of the public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn func_by_index(&self, store: &Store, func_index: FuncAddr) -> Result<FuncHandle> {
        self.validate_store(store)?;
        let func_addr = Self::index_addr(&self.0.func_addrs, func_index, "function")?;

        let ty = store.state.get_func(func_addr).func.ty();
        Ok(FuncHandle { store_id: self.0.store_id, addr: func_addr, module_addr: self.id(), ty: ty.clone() })
    }

    /// Get a typed function export by name.
    pub fn func_typed<P: IntoWasmValueTuple + ValTypesFromTuple, R: FromWasmValueTuple + ValTypesFromTuple>(
        &self,
        store: &Store,
        name: &str,
    ) -> Result<FuncHandleTyped<P, R>> {
        let func = self.func(store, name)?;
        Self::validate_typed_func::<P, R>(&func, name)?;
        Ok(FuncHandleTyped { func, marker: core::marker::PhantomData })
    }

    /// Get a typed function by its module-local index.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn func_typed_by_index<P: IntoWasmValueTuple + ValTypesFromTuple, R: FromWasmValueTuple + ValTypesFromTuple>(
        &self,
        store: &Store,
        func_index: FuncAddr,
    ) -> Result<FuncHandleTyped<P, R>> {
        let func = self.func_by_index(store, func_index)?;
        Self::validate_typed_func::<P, R>(&func, &format!("function index {func_index}"))?;
        Ok(FuncHandleTyped { func, marker: core::marker::PhantomData })
    }

    fn validate_typed_func<P: ValTypesFromTuple, R: ValTypesFromTuple>(
        func: &FuncHandle,
        func_name: &str,
    ) -> Result<()> {
        let expected = FuncType { params: P::val_types(), results: R::val_types() };
        if func.ty != expected {
            return Err(Error::Other(format!(
                "function type mismatch for {func_name}: expected {expected:?}, actual {:?}",
                func.ty
            )));
        }

        Ok(())
    }

    /// Get a memory export by name.
    pub fn memory<'a>(&self, store: &'a Store, name: &str) -> Result<MemoryRef<'a>> {
        self.validate_store(store)?;

        let export = self.require_export(name)?;
        let ExternVal::Memory(mem_addr) = export else {
            return Err(Error::Other(format!("Export is not a memory: {name}")));
        };
        Ok(MemoryRef(store.state.get_mem(mem_addr)))
    }

    /// Get a mutable memory export by name.
    pub fn memory_mut<'a>(&self, store: &'a mut Store, name: &str) -> Result<MemoryRefMut<'a>> {
        self.validate_store(store)?;

        let export = self.require_export(name)?;
        let ExternVal::Memory(mem_addr) = export else {
            return Err(Error::Other(format!("Export is not a memory: {name}")));
        };
        Ok(MemoryRefMut(store.state.get_mem_mut(mem_addr)))
    }

    /// Get a memory by its module-local index.
    ///
    /// This exposes an internal module-owned memory directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private memory can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn memory_by_index<'a>(&self, store: &'a Store, memory_index: MemAddr) -> Result<MemoryRef<'a>> {
        self.validate_store(store)?;
        let mem_addr = Self::index_addr(&self.0.mem_addrs, memory_index, "memory")?;
        Ok(MemoryRef(store.state.get_mem(mem_addr)))
    }

    /// Get a mutable memory by its module-local index.
    ///
    /// This exposes an internal module-owned memory directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private memory can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn memory_mut_by_index<'a>(&self, store: &'a mut Store, memory_index: MemAddr) -> Result<MemoryRefMut<'a>> {
        self.validate_store(store)?;
        let mem_addr = Self::index_addr(&self.0.mem_addrs, memory_index, "memory")?;
        Ok(MemoryRefMut(store.state.get_mem_mut(mem_addr)))
    }

    /// Get a table export by name.
    pub fn table<'a>(&self, store: &'a Store, name: &str) -> Result<TableRef<'a>> {
        self.validate_store(store)?;

        let export = self.require_export(name)?;
        let ExternVal::Table(table_addr) = export else {
            return Err(Error::Other(format!("Export is not a table: {name}")));
        };
        Ok(TableRef(store.state.get_table(table_addr)))
    }

    /// Get a mutable table export by name.
    pub fn table_mut<'a>(&self, store: &'a mut Store, name: &str) -> Result<TableRefMut<'a>> {
        self.validate_store(store)?;

        let export = self.require_export(name)?;
        let ExternVal::Table(table_addr) = export else {
            return Err(Error::Other(format!("Export is not a table: {name}")));
        };
        Ok(TableRefMut(store.state.get_table_mut(table_addr)))
    }

    /// Get a table by its module-local index.
    ///
    /// This exposes an internal module-owned table directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private table can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn table_by_index<'a>(&self, store: &'a Store, table_index: TableAddr) -> Result<TableRef<'a>> {
        self.validate_store(store)?;
        let table_addr = Self::index_addr(&self.0.table_addrs, table_index, "table")?;
        Ok(TableRef(store.state.get_table(table_addr)))
    }

    /// Get a mutable table by its module-local index.
    ///
    /// This exposes an internal module-owned table directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private table can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn table_mut_by_index<'a>(&self, store: &'a mut Store, table_index: TableAddr) -> Result<TableRefMut<'a>> {
        self.validate_store(store)?;
        let table_addr = Self::index_addr(&self.0.table_addrs, table_index, "table")?;
        Ok(TableRefMut(store.state.get_table_mut(table_addr)))
    }

    /// Get the value of a global export by name.
    pub fn global_get(&self, store: &Store, name: &str) -> Result<WasmValue> {
        self.global(store, name).map(|global| global.get())
    }

    /// Get a reference to a global export by name.
    pub fn global<'a>(&self, store: &'a Store, name: &str) -> Result<GlobalRef<'a>> {
        self.validate_store(store)?;

        let export = self.require_export(name)?;
        let ExternVal::Global(global_addr) = export else {
            return Err(Error::Other(format!("Export is not a global: {name}")));
        };

        Ok(GlobalRef(store.state.get_global(global_addr)))
    }

    /// Get a mutable reference to a global export by name.
    pub fn global_mut<'a>(&self, store: &'a mut Store, name: &str) -> Result<GlobalRefMut<'a>> {
        self.validate_store(store)?;

        let export = self.require_export(name)?;
        let ExternVal::Global(global_addr) = export else {
            return Err(Error::Other(format!("Export is not a global: {name}")));
        };

        Ok(GlobalRefMut(store.state.get_global_mut(global_addr)))
    }

    /// Set the value of a mutable global export by name.
    pub fn global_set(&self, store: &mut Store, name: &str, value: WasmValue) -> Result<()> {
        self.global_mut(store, name)?.set(value)
    }

    /// Get a reference to a global by its module-local index.
    ///
    /// This exposes an internal module-owned global directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private global can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn global_by_index<'a>(&self, store: &'a Store, global_index: GlobalAddr) -> Result<GlobalRef<'a>> {
        self.validate_store(store)?;
        let global_addr = Self::index_addr(&self.0.global_addrs, global_index, "global")?;

        Ok(GlobalRef(store.state.get_global(global_addr)))
    }

    /// Get a mutable reference to a global by its module-local index.
    ///
    /// This exposes an internal module-owned global directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private global can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest_debug")))]
    #[cfg(feature = "guest_debug")]
    pub fn global_mut_by_index<'a>(&self, store: &'a mut Store, global_index: GlobalAddr) -> Result<GlobalRefMut<'a>> {
        self.validate_store(store)?;
        let global_addr = Self::index_addr(&self.0.global_addrs, global_index, "global")?;

        Ok(GlobalRefMut(store.state.get_global_mut(global_addr)))
    }

    /// Get the start function of the module
    ///
    /// Returns None if the module has no start function
    /// If no start function is specified, also checks for a `_start` function in the exports
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#start-function>
    pub fn start_func(&self, store: &Store) -> Result<Option<FuncHandle>> {
        self.validate_store(store)?;

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

        let func_addr = self.0.resolve_func_addr(func_index);
        let ty = store.state.get_func(func_addr).func.ty();
        Ok(Some(FuncHandle { store_id: self.0.store_id, module_addr: self.id(), addr: func_addr, ty: ty.clone() }))
    }

    /// Invoke the start function of the module
    ///
    /// Returns `None` if the module has no start function
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start>
    pub fn start(&self, store: &mut Store) -> Result<Option<()>> {
        let Some(func) = self.start_func(store)? else {
            return Ok(None);
        };
        func.call(store, &[]).map(|_| Some(()))
    }
}
