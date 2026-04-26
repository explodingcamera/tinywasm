use core::hint::cold_path;

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::{format, rc::Rc};
use tinywasm_types::*;

use crate::func::{FromWasmValueTuple, IntoWasmValueTuple, WasmTypesFromTuple};
use crate::{Error, Function, FunctionTyped, Global, Imports, Memory, Result, Store, Table};

/// A typed view over an exported extern value.
pub enum ExternItem {
    /// Exported function handle.
    Func(Function),
    /// Exported memory reference.
    Memory(Memory),
    /// Exported table reference.
    Table(Table),
    /// Exported global reference.
    Global(Global),
}

/// An instantiated WebAssembly module
///
/// Backed by an Rc, so cloning is cheap
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#module-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct ModuleInstance(Rc<ModuleInstanceInner>);

#[cfg_attr(feature = "debug", derive(Debug))]
struct ModuleInstanceInner {
    store_id: usize,
    idx: ModuleInstanceAddr,
    types: Arc<[Arc<FuncType>]>,
    func_addrs: Box<[FuncAddr]>,
    table_addrs: Box<[TableAddr]>,
    mem_addrs: Box<[MemAddr]>,
    global_addrs: Box<[GlobalAddr]>,
    elem_addrs: Box<[ElemAddr]>,
    data_addrs: Box<[DataAddr]>,
    func_start: Option<FuncAddr>,
    exports: Arc<[Export]>,
}

impl ModuleInstance {
    #[inline]
    pub(crate) fn idx(&self) -> ModuleInstanceAddr {
        self.0.idx
    }

    #[inline]
    pub(crate) fn func_ty(&self, addr: FuncAddr) -> &Arc<FuncType> {
        match self.0.types.get(addr as usize) {
            Some(ty) => ty,
            None => {
                cold_path();
                unreachable!("invalid function address: {addr}")
            }
        }
    }

    #[inline]
    pub(crate) fn func_addrs(&self) -> &[FuncAddr] {
        &self.0.func_addrs
    }

    // resolve a function address to the global store address
    #[inline]
    pub(crate) fn resolve_func_addr(&self, addr: FuncAddr) -> FuncAddr {
        match self.0.func_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => {
                cold_path();
                unreachable!("invalid function address: {addr}")
            }
        }
    }

    // resolve a table address to the global store address
    #[inline]
    pub(crate) fn resolve_table_addr(&self, addr: TableAddr) -> TableAddr {
        match self.0.table_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => {
                cold_path();
                unreachable!("invalid table address: {addr}")
            }
        }
    }

    // resolve a memory address to the global store address
    #[inline]
    pub(crate) fn resolve_mem_addr(&self, addr: MemAddr) -> MemAddr {
        match self.0.mem_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => {
                cold_path();
                unreachable!("invalid memory address: {addr}")
            }
        }
    }

    // resolve a data address to the global store address
    #[inline]
    pub(crate) fn resolve_data_addr(&self, addr: DataAddr) -> DataAddr {
        match self.0.data_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => {
                cold_path();
                unreachable!("invalid data address: {addr}")
            }
        }
    }

    // resolve an element address to the global store address
    #[inline]
    pub(crate) fn resolve_elem_addr(&self, addr: ElemAddr) -> ElemAddr {
        match self.0.elem_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => {
                cold_path();
                unreachable!("invalid element address: {addr}")
            }
        }
    }

    // resolve a global address to the global store address
    #[inline]
    pub(crate) fn resolve_global_addr(&self, addr: GlobalAddr) -> GlobalAddr {
        match self.0.global_addrs.get(addr as usize) {
            Some(addr) => *addr,
            None => {
                cold_path();
                unreachable!("invalid global address: {addr}")
            }
        }
    }

    #[inline]
    pub(crate) fn validate_store(&self, store: &Store) -> Result<()> {
        if self.0.store_id != store.id() {
            cold_path();
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
    pub fn instantiate(store: &mut Store, module: &Module, imports: Option<Imports>) -> Result<Self> {
        let instance = ModuleInstance::instantiate_no_start(store, module, imports)?;
        let _ = instance.start(store)?;
        Ok(instance)
    }

    /// Instantiate the module in the given store (without running the start function)
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation>
    pub fn instantiate_no_start(store: &mut Store, module: &Module, imports: Option<Imports>) -> Result<Self> {
        let idx = store.next_module_instance_idx();
        let mut addrs = imports.unwrap_or_default().link(store, module)?;

        addrs.funcs.extend(store.init_funcs(&module.funcs, idx));
        addrs.tables.extend(store.init_tables(&module.table_types));
        match module.local_memory_allocation {
            LocalMemoryAllocation::Skip => {}
            LocalMemoryAllocation::Lazy => addrs.memories.extend(store.init_lazy_memories(&module.memory_types)?),
            LocalMemoryAllocation::Eager => addrs.memories.extend(store.init_memories(&module.memory_types)?),
        }

        store.init_globals(&mut addrs.globals, &module.globals, &addrs.funcs)?;
        let (elem_addrs, elem_trapped) =
            store.init_elements(&addrs.tables, &addrs.funcs, &addrs.globals, &module.elements)?;
        let (data_addrs, data_trapped) =
            store.init_data(&addrs.memories, &addrs.globals, &addrs.funcs, &module.data)?;

        let instance = ModuleInstanceInner {
            store_id: store.id(),
            idx,
            types: module.func_types.clone(),
            func_addrs: addrs.funcs.into_boxed_slice(),
            table_addrs: addrs.tables.into_boxed_slice(),
            mem_addrs: addrs.memories.into_boxed_slice(),
            global_addrs: addrs.globals.into_boxed_slice(),
            elem_addrs,
            data_addrs,
            func_start: module.start_func,
            exports: module.exports.clone(),
        };

        let instance = ModuleInstance(Rc::new(instance));
        store.add_instance(instance.clone());

        match (elem_trapped, data_trapped) {
            (Some(trap), _) | (_, Some(trap)) => {
                cold_path();
                Err(trap.into())
            }
            _ => Ok(instance),
        }
    }

    /// Get a export by name
    pub fn export_addr(&self, name: &str) -> Option<ExternVal> {
        let exports = self.0.exports.iter().find(|e| *e.name == *name)?;
        let addr = match exports.kind {
            ExternalKind::Func => self.0.func_addrs.get(exports.index as usize)?,
            ExternalKind::Table => self.0.table_addrs.get(exports.index as usize)?,
            ExternalKind::Memory => self.0.mem_addrs.get(exports.index as usize)?,
            ExternalKind::Global => self.0.global_addrs.get(exports.index as usize)?,
        };
        Some(ExternVal::new(exports.kind, *addr))
    }

    /// Returns an iterator over all exported extern values for this instance.
    pub fn exports(&self) -> impl Iterator<Item = (&str, ExternItem)> + '_ {
        self.0.exports.iter().map(move |export| {
            let item = match export.kind {
                ExternalKind::Func => {
                    let func_addr = self.resolve_func_addr(export.index);
                    ExternItem::Func(Function {
                        item: crate::StoreItem::new(self.0.store_id, func_addr),
                        module_addr: self.id(),
                        addr: func_addr,
                        ty: self.func_ty(export.index).clone(),
                    })
                }
                ExternalKind::Table => {
                    ExternItem::Table(Table::from_store_addr(self.0.store_id, self.resolve_table_addr(export.index)))
                }
                ExternalKind::Memory => {
                    ExternItem::Memory(Memory::from_store_addr(self.0.store_id, self.resolve_mem_addr(export.index)))
                }
                ExternalKind::Global => {
                    ExternItem::Global(Global::from_store_addr(self.0.store_id, self.resolve_global_addr(export.index)))
                }
            };

            (export.name.as_ref(), item)
        })
    }

    #[inline]
    fn require_export(&self, name: &str) -> Result<ExternVal> {
        match self.export_addr(name) {
            Some(addr) => Ok(addr),
            None => {
                cold_path();
                Err(Error::Other(format!("Export not found: {name}")))
            }
        }
    }

    #[inline]
    #[cfg(feature = "guest-debug")]
    fn index_addr<T: Copy>(slice: &[T], idx: u32, kind: &str) -> Result<T> {
        match slice.get(idx as usize) {
            Some(addr) => Ok(*addr),
            None => {
                cold_path();
                Err(Error::Other(format!("{kind} index out of bounds: {idx}")))
            }
        }
    }

    /// Get any exported extern value by name.
    pub fn extern_item(&self, name: &str) -> Result<ExternItem> {
        match self.require_export(name)? {
            ExternVal::Func(addr) => {
                let export = self.0.exports.iter().find(|e| e.name == name.into());
                let export = export.ok_or_else(|| Error::Other(format!("Export not found: {name}")))?;
                Ok(ExternItem::Func(Function {
                    item: crate::StoreItem::new(self.0.store_id, addr),
                    module_addr: self.id(),
                    addr,
                    ty: self.func_ty(export.index).clone(),
                }))
            }
            ExternVal::Memory(addr) => Ok(ExternItem::Memory(Memory::from_store_addr(self.0.store_id, addr))),
            ExternVal::Table(addr) => Ok(ExternItem::Table(Table::from_store_addr(self.0.store_id, addr))),
            ExternVal::Global(addr) => Ok(ExternItem::Global(Global::from_store_addr(self.0.store_id, addr))),
        }
    }

    /// Get a function export by name.
    pub fn func_untyped(&self, store: &Store, name: &str) -> Result<Function> {
        self.validate_store(store)?;

        let func_addr = match self.require_export(name)? {
            ExternVal::Func(func_addr) => func_addr,
            _ => {
                cold_path();
                return Err(Error::Other(format!("Export is not a function: {name}")));
            }
        };

        Ok(Function {
            item: crate::StoreItem::new(self.0.store_id, func_addr),
            addr: func_addr,
            module_addr: self.id(),
            ty: store.state.get_func(func_addr).ty().clone(),
        })
    }

    /// Get a function by its module-local index.
    ///
    /// This exposes an internal module-owned function directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// introspection. Calling private functions can change behavior in ways the
    /// module author did not expose as part of the public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest-debug")))]
    #[cfg(feature = "guest-debug")]
    pub fn func_by_index(&self, store: &Store, func_index: FuncAddr) -> Result<Function> {
        self.validate_store(store)?;
        let func_addr = Self::index_addr(&self.0.func_addrs, func_index, "function")?;

        let ty = store.state.get_func(func_addr).ty();
        Ok(Function {
            item: crate::StoreItem::new(self.0.store_id, func_addr),
            addr: func_addr,
            module_addr: self.id(),
            ty: ty.clone(),
        })
    }

    /// Get a typed function export by name.
    pub fn func<P: IntoWasmValueTuple + WasmTypesFromTuple, R: FromWasmValueTuple + WasmTypesFromTuple>(
        &self,
        store: &Store,
        name: &str,
    ) -> Result<FunctionTyped<P, R>> {
        let func = self.func_untyped(store, name)?;
        Self::validate_typed_func::<P, R>(&func, name)?;
        Ok(FunctionTyped { func, marker: core::marker::PhantomData })
    }

    /// Get a typed function by its module-local index.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest-debug")))]
    #[cfg(feature = "guest-debug")]
    pub fn func_typed_by_index<
        P: IntoWasmValueTuple + WasmTypesFromTuple,
        R: FromWasmValueTuple + WasmTypesFromTuple,
    >(
        &self,
        store: &Store,
        func_index: FuncAddr,
    ) -> Result<FunctionTyped<P, R>> {
        let func = self.func_by_index(store, func_index)?;
        Self::validate_typed_func::<P, R>(&func, &format!("function index {func_index}"))?;
        Ok(FunctionTyped { func, marker: core::marker::PhantomData })
    }

    fn validate_typed_func<P: WasmTypesFromTuple, R: WasmTypesFromTuple>(
        func: &Function,
        func_name: &str,
    ) -> Result<()> {
        if *func.ty.params() != *P::wasm_types() || *func.ty.results() != *R::wasm_types() {
            cold_path();

            #[cfg(feature = "debug")]
            return Err(Error::Other(format!(
                "function type mismatch for {func_name}: expected {:?}, actual {:?}",
                FuncType::new(&P::wasm_types(), &R::wasm_types()),
                func.ty
            )));

            #[cfg(not(feature = "debug"))]
            return Err(Error::Other(format!("function type mismatch for {func_name}")));
        }

        Ok(())
    }

    /// Get a memory export by name.
    pub fn memory(&self, name: &str) -> Result<Memory> {
        match self.require_export(name)? {
            ExternVal::Memory(mem_addr) => Ok(Memory::from_store_addr(self.0.store_id, mem_addr)),
            _ => {
                cold_path();
                Err(Error::Other(format!("Export is not a memory: {name}")))
            }
        }
    }

    /// Get a memory by its module-local index.
    ///
    /// This exposes an internal module-owned memory directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private memory can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest-debug")))]
    #[cfg(feature = "guest-debug")]
    pub fn memory_by_index(&self, memory_index: MemAddr) -> Result<Memory> {
        Ok(Memory::from_store_addr(self.0.store_id, Self::index_addr(&self.0.mem_addrs, memory_index, "memory")?))
    }

    /// Get a table export by name.
    pub fn table(&self, name: &str) -> Result<Table> {
        match self.require_export(name)? {
            ExternVal::Table(table_addr) => Ok(Table::from_store_addr(self.0.store_id, table_addr)),
            _ => Err(Error::Other(format!("Export is not a table: {name}"))),
        }
    }

    /// Get a table by its module-local index.
    ///
    /// This exposes an internal module-owned table directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private table can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest-debug")))]
    #[cfg(feature = "guest-debug")]
    pub fn table_by_index(&self, table_index: TableAddr) -> Result<Table> {
        Ok(Table::from_store_addr(self.0.store_id, Self::index_addr(&self.0.table_addrs, table_index, "table")?))
    }

    /// Get the value of a global export by name.
    pub fn global_get(&self, store: &Store, name: &str) -> Result<WasmValue> {
        self.global(name)?.get(store)
    }

    /// Get a global export by name.
    pub fn global(&self, name: &str) -> Result<Global> {
        match self.require_export(name)? {
            ExternVal::Global(global_addr) => Ok(Global::from_store_addr(self.0.store_id, global_addr)),
            _ => Err(Error::Other(format!("Export is not a global: {name}"))),
        }
    }

    /// Set the value of a mutable global export by name.
    pub fn global_set(&self, store: &mut Store, name: &str, value: WasmValue) -> Result<()> {
        self.global(name)?.set(store, value)
    }

    /// Get a global by its module-local index.
    ///
    /// This exposes an internal module-owned global directly and bypasses the
    /// normal export boundary. It is mainly intended for tooling and
    /// inspection. Mutating a private global can change module behavior in ways
    /// that are not part of the module's public API.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest-debug")))]
    #[cfg(feature = "guest-debug")]
    pub fn global_by_index(&self, global_index: GlobalAddr) -> Result<Global> {
        Ok(Global::from_store_addr(self.0.store_id, Self::index_addr(&self.0.global_addrs, global_index, "global")?))
    }

    /// Get the start function of the module
    ///
    /// Returns None if the module has no start function
    /// If no start function is specified, also checks for a `_start` function in the exports
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#start-function>
    pub fn start_func(&self, store: &Store) -> Result<Option<Function>> {
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

        let func_addr = self.resolve_func_addr(func_index);
        Ok(Some(Function {
            item: crate::StoreItem::new(self.0.store_id, func_addr),
            module_addr: self.id(),
            addr: func_addr,
            ty: store.state.get_func(func_addr).ty().clone(),
        }))
    }

    /// Invoke the start function of the module
    ///
    /// Returns `None` if the module has no start function
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start>
    pub fn start(&self, store: &mut Store) -> Result<Option<()>> {
        match self.start_func(store)? {
            Some(func) => func.call(store, &[]).map(|_| Some(())),
            None => Ok(None),
        }
    }
}
