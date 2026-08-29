use alloc::{boxed::Box, format};
use core::hint::cold_path;
use tinywasm_types::*;

use crate::func::{FromWasmValues, IntoWasmValues, WasmTypes};
use crate::reference::StoreId;
use crate::shared::StoreShared;
use crate::store::MemoryInstance;
use crate::{
    Error, Function, FunctionTyped, Global, Imports, Memory, Result, Store, StoreItem, Table, Tag, Trap, WasmValue,
};

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
    /// Exported tag reference.
    Tag(Tag),
}

/// An instantiated WebAssembly module
///
/// Create a `ModuleInstance` by parsing a module, creating a [`Store`], and then calling
/// [`ModuleInstance::instantiate`].
///
/// ## Example
/// ```rust
/// # fn main() -> tinywasm::Result<()> {
/// # use tinywasm::{ModuleInstance, Store};
/// # let wasm = wat::parse_str("(module)").expect("valid wat");
/// let module = tinywasm::parse_bytes(&wasm)?;
/// let mut store = Store::default();
/// let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
/// # let _ = instance;
/// # Ok(())
/// # }
/// ```
///
/// Cloning an instance handle is cheap.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#module-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct ModuleInstance(StoreShared<ModuleInstanceInner>);

#[cfg_attr(feature = "debug", derive(Debug))]
struct ModuleInstanceInner {
    store_id: StoreId,
    id: ModuleInstanceId,
    type_addrs: Box<[TypeAddr]>,
    func_addrs: Box<[FuncAddr]>,
    table_addrs: Box<[TableAddr]>,
    mem_addrs: Box<[MemAddr]>,
    global_addrs: Box<[GlobalAddr]>,
    tag_addrs: Box<[TagAddr]>,
    elem_addrs: Box<[ElemAddr]>,
    data_addrs: Box<[DataAddr]>,
    func_start: Option<FuncAddr>,
    exports: Shared<[Export]>,
}

impl ModuleInstance {
    #[inline]
    pub(crate) fn resolve_type_addr(&self, type_addr: TypeAddr) -> TypeAddr {
        self.0.type_addrs[type_addr as usize]
    }

    /// resolve a function address to the global store address
    #[inline]
    pub(crate) fn resolve_func_addr(&self, addr: FuncAddr) -> FuncAddr {
        self.0.func_addrs[addr as usize]
    }

    /// resolve a table address to the global store address
    #[inline]
    pub(crate) fn resolve_table_addr(&self, addr: TableAddr) -> TableAddr {
        self.0.table_addrs[addr as usize]
    }

    /// resolve a memory address to the global store address
    #[inline]
    pub(crate) fn resolve_mem_addr(&self, addr: MemAddr) -> MemAddr {
        self.0.mem_addrs[addr as usize]
    }

    /// The resolved store address of the module's first memory, or `MemAddr::MAX` when the module
    /// has none.
    ///
    /// The sentinel is never used in practice: validation guarantees a memory instruction can only
    /// reference index 0 when the module declares a memory, so any read of the sentinel would trip
    /// an out-of-bounds panic in the store lookup and surface a bug.
    #[inline]
    pub(crate) fn mem0_addr(&self) -> MemAddr {
        self.0.mem_addrs.first().copied().unwrap_or(MemAddr::MAX)
    }

    /// resolve a data address to the global store address
    #[inline]
    pub(crate) fn resolve_data_addr(&self, addr: DataAddr) -> DataAddr {
        self.0.data_addrs[addr as usize]
    }

    /// resolve an element address to the global store address
    #[inline]
    pub(crate) fn resolve_elem_addr(&self, addr: ElemAddr) -> ElemAddr {
        self.0.elem_addrs[addr as usize]
    }

    /// resolve a global address to the global store address
    #[inline]
    pub(crate) fn resolve_global_addr(&self, addr: GlobalAddr) -> GlobalAddr {
        self.0.global_addrs[addr as usize]
    }

    #[inline]
    pub(crate) fn resolve_tag_addr(&self, addr: TagAddr) -> TagAddr {
        self.0.tag_addrs[addr as usize]
    }

    #[inline]
    pub(crate) fn validate_store(&self, store: &Store) -> Result<()> {
        if self.0.store_id != store.id() {
            return cold!(Err(Trap::InvalidStore.into()));
        }
        Ok(())
    }

    /// Get the module instance's address
    pub fn id(&self) -> ModuleInstanceId {
        self.0.id
    }

    /// Instantiate the module in the given store
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation>
    pub fn instantiate(store: &mut Store, module: &Module, imports: Option<&Imports>) -> Result<Self> {
        let instance = ModuleInstance::instantiate_no_start(store, module, imports)?;
        let _ = instance.start(store)?;
        Ok(instance)
    }

    /// Instantiate the module in the given store (without running the start function)
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{ModuleInstance, Store};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (global $g (mut i32) (i32.const 0))
    /// #       (func $start
    /// #         i32.const 42
    /// #         global.set $g)
    /// #       (start $start)
    /// #       (export "g" (global $g)))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate_no_start(&mut store, &module, None)?;
    ///
    /// assert_eq!(instance.global_get(&mut store, "g")?, 0.into());
    /// instance.start(&mut store)?;
    /// assert_eq!(instance.global_get(&mut store, "g")?, 42.into());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// See <https://webassembly.github.io/spec/core/exec/modules.html#exec-instantiation>
    pub fn instantiate_no_start(store: &mut Store, module: &Module, imports: Option<&Imports>) -> Result<Self> {
        let type_addrs = store.register_module_types(&module.types);
        let id = store.next_module_instance_id();
        let default_imports = Imports::default();
        let mut addrs = imports.unwrap_or(&default_imports).link(store, module, &type_addrs)?;
        let imported_funcs = addrs.funcs.len();
        addrs.funcs.extend(store.init_funcs(&module.funcs, id, &module.func_type_idxs[imported_funcs..], &type_addrs));
        addrs.tags.extend(store.init_tags(&module.tags, &type_addrs));
        let limiter = store.engine.config().resource_limiter.clone();
        if !module.skip_local_memory_allocation {
            addrs
                .memories
                .extend(store.init_memories(&module.memory_types, |ty| MemoryInstance::new(ty, limiter.as_deref()))?);
        }

        store.init_globals(&mut addrs.globals, &module.globals, &addrs.funcs, &type_addrs)?;
        addrs.tables.extend(store.init_tables(&module.tables, &addrs.globals, &addrs.funcs, &type_addrs)?);
        let (elem_addrs, elem_trapped) =
            store.init_elements(&addrs.tables, &addrs.funcs, &addrs.globals, &module.elements, &type_addrs)?;
        let (data_addrs, data_trapped) =
            store.init_data(&addrs.memories, &addrs.globals, &addrs.funcs, &module.data, &type_addrs)?;

        let instance = ModuleInstanceInner {
            store_id: store.id(),
            id,
            type_addrs,
            func_addrs: addrs.funcs.into_boxed_slice(),
            table_addrs: addrs.tables.into_boxed_slice(),
            mem_addrs: addrs.memories.into_boxed_slice(),
            global_addrs: addrs.globals.into_boxed_slice(),
            tag_addrs: addrs.tags.into_boxed_slice(),
            elem_addrs,
            data_addrs,
            func_start: module.start_func,
            exports: module.exports.clone(),
        };

        let instance = ModuleInstance(StoreShared::new(instance));
        store.add_instance(instance.clone());

        if let Some(trap) = elem_trapped.or(data_trapped) {
            return cold!(Err(trap.into()));
        }
        Ok(instance)
    }

    /// Get a export by name
    pub(crate) fn export_addr(&self, name: &str) -> Option<ExternVal> {
        let export = self.0.exports.iter().find(|e| *e.name == *name)?;
        let addr = match export.kind {
            ExternalKind::Func => self.0.func_addrs.get(export.index as usize)?,
            ExternalKind::Table => self.0.table_addrs.get(export.index as usize)?,
            ExternalKind::Memory => self.0.mem_addrs.get(export.index as usize)?,
            ExternalKind::Global => self.0.global_addrs.get(export.index as usize)?,
            ExternalKind::Tag => self.0.tag_addrs.get(export.index as usize)?,
        };
        Some(ExternVal::new(export.kind, *addr))
    }

    /// Returns an iterator over all exported extern values for this instance.
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{ExternItem, ModuleInstance, Store};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (func (export "f"))
    /// #       (memory (export "mem") 1))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// # let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    ///
    /// let mut saw_func = false;
    /// let mut saw_memory = false;
    /// for (name, item) in instance.exports() {
    ///     match (name, item) {
    ///         ("f", ExternItem::Func(_)) => saw_func = true,
    ///         ("mem", ExternItem::Memory(_)) => saw_memory = true,
    ///         _ => {}
    ///     }
    /// }
    /// assert!(saw_func && saw_memory);
    /// # Ok(())
    /// # }
    /// ```
    pub fn exports(&self) -> impl Iterator<Item = (&str, ExternItem)> + '_ {
        self.0.exports.iter().map(move |export| {
            let item = match export.kind {
                ExternalKind::Func => ExternItem::Func(Function {
                    item: StoreItem::new(self.0.store_id, self.resolve_func_addr(export.index)),
                    module_id: self.id(),
                }),
                ExternalKind::Table => {
                    ExternItem::Table(Table(StoreItem::new(self.0.store_id, self.resolve_table_addr(export.index))))
                }
                ExternalKind::Memory => {
                    ExternItem::Memory(Memory(StoreItem::new(self.0.store_id, self.resolve_mem_addr(export.index))))
                }
                ExternalKind::Global => {
                    ExternItem::Global(Global(StoreItem::new(self.0.store_id, self.resolve_global_addr(export.index))))
                }
                ExternalKind::Tag => {
                    ExternItem::Tag(Tag(StoreItem::new(self.0.store_id, self.resolve_tag_addr(export.index))))
                }
            };

            (export.name.as_ref(), item)
        })
    }

    #[inline]
    fn require_export(&self, name: &str) -> Result<ExternVal> {
        self.export_addr(name).ok_or_else(|| {
            cold_path();
            Error::Other(format!("Export not found: {name}"))
        })
    }

    #[inline]
    #[cfg(feature = "guest-debug")]
    fn index_addr<T: Copy>(slice: &[T], idx: u32, kind: &str) -> Result<T> {
        slice.get(idx as usize).copied().ok_or_else(|| {
            cold_path();
            Error::Other(format!("{kind} index out of bounds: {idx}"))
        })
    }

    /// Get any exported extern value by name.
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{ExternItem, ModuleInstance, Store};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (global (export "answer") i32 (i32.const 42)))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// # let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    ///
    /// let ExternItem::Global(global) = instance.extern_item("answer")? else {
    ///     panic!("expected global export");
    /// };
    /// assert_eq!(global.get(&mut store)?, 42.into());
    /// # Ok(())
    /// # }
    /// ```
    pub fn extern_item(&self, name: &str) -> Result<ExternItem> {
        match self.require_export(name)? {
            ExternVal::Func(addr) => {
                Ok(ExternItem::Func(Function { item: StoreItem::new(self.0.store_id, addr), module_id: self.id() }))
            }
            ExternVal::Memory(addr) => Ok(ExternItem::Memory(Memory(StoreItem::new(self.0.store_id, addr)))),
            ExternVal::Table(addr) => Ok(ExternItem::Table(Table(StoreItem::new(self.0.store_id, addr)))),
            ExternVal::Global(addr) => Ok(ExternItem::Global(Global(StoreItem::new(self.0.store_id, addr)))),
            ExternVal::Tag(addr) => Ok(ExternItem::Tag(Tag(StoreItem::new(self.0.store_id, addr)))),
        }
    }

    /// Get a function export by name.
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{ModuleInstance, Store};
    /// # use tinywasm::types::WasmValue;
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (func (export "add") (param i32 i32) (result i32)
    /// #         local.get 0
    /// #         local.get 1
    /// #         i32.add))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// # let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    /// let add = instance.func_untyped(&store, "add")?;
    /// let mut results = [WasmValue::I32(0)];
    /// add.call(&mut store, &[WasmValue::I32(20), WasmValue::I32(22)], &mut results)?;
    /// assert_eq!(results, [WasmValue::I32(42)]);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For typed access, see [`Self::func`].
    pub fn func_untyped(&self, store: &Store, name: &str) -> Result<Function> {
        self.validate_store(store)?;
        let ExternVal::Func(func_addr) = self.require_export(name)? else {
            return cold!(Err(Error::Other(format!("Export is not a function: {name}"))));
        };

        Ok(Function { item: StoreItem::new(self.0.store_id, func_addr), module_id: self.id() })
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
        Ok(Function { item: StoreItem::new(self.0.store_id, func_addr), module_id: self.id() })
    }

    /// Get a typed function export by name.
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{ModuleInstance, Store};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (func (export "add") (param i32 i32) (result i32)
    /// #         local.get 0
    /// #         local.get 1
    /// #         i32.add))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// # let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    /// let add = instance.func::<(i32, i32), i32>(&store, "add")?;
    /// assert_eq!(add.call(&mut store, (20, 22))?, 42);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For untyped access, see [`Self::func_untyped`] and [`Self::extern_item`].
    ///
    /// Tuples are supported up to arity 20. Use [`Self::func_untyped`] for larger signatures.
    pub fn func<P: IntoWasmValues, R: FromWasmValues>(&self, store: &Store, name: &str) -> Result<FunctionTyped<P, R>> {
        self.validate_store(store)?;
        let ExternVal::Func(func_addr) = self.require_export(name)? else {
            return cold!(Err(Error::Other(format!("Export is not a function: {name}"))));
        };

        let func = Function { item: StoreItem::new(self.0.store_id, func_addr), module_id: self.id() };
        Self::validate_typed_func::<P, R>(store, &func, name)?;
        Ok(FunctionTyped { func, marker: core::marker::PhantomData })
    }

    /// Get a typed function by its module-local index.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest-debug")))]
    #[cfg(feature = "guest-debug")]
    pub fn func_typed_by_index<P: IntoWasmValues, R: FromWasmValues>(
        &self,
        store: &Store,
        func_index: FuncAddr,
    ) -> Result<FunctionTyped<P, R>> {
        let func = self.func_by_index(store, func_index)?;
        Self::validate_typed_func::<P, R>(store, &func, &format!("function index {func_index}"))?;
        Ok(FunctionTyped { func, marker: core::marker::PhantomData })
    }

    #[inline]
    fn validate_typed_func<P: WasmTypes, R: WasmTypes>(store: &Store, func: &Function, func_name: &str) -> Result<()> {
        let params = P::WASM_TYPES;
        let results = R::WASM_TYPES;
        let ty = store.state.get_func_type(func.addr());
        if ty.params() != params || ty.results() != results {
            cold_path();

            #[cfg(feature = "debug")]
            return Err(Error::Other(format!(
                "function type mismatch for {func_name}: expected {:?}, actual {:?}",
                FuncType::new(params, results),
                ty
            )));

            #[cfg(not(feature = "debug"))]
            return Err(Error::Other(format!("function type mismatch for {func_name}")));
        }

        Ok(())
    }

    /// Get a memory export by name.
    pub fn memory(&self, name: &str) -> Result<Memory> {
        match self.require_export(name)? {
            ExternVal::Memory(mem_addr) => Ok(Memory(StoreItem::new(self.0.store_id, mem_addr))),
            _ => cold!(Err(Error::Other(format!("Export is not a memory: {name}")))),
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
        Ok(Memory(StoreItem::new(self.0.store_id, Self::index_addr(&self.0.mem_addrs, memory_index, "memory")?)))
    }

    /// Get a table export by name.
    pub fn table(&self, name: &str) -> Result<Table> {
        match self.require_export(name)? {
            ExternVal::Table(table_addr) => Ok(Table(StoreItem::new(self.0.store_id, table_addr))),
            _ => cold!(Err(Error::Other(format!("Export is not a table: {name}")))),
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
        Ok(Table(StoreItem::new(self.0.store_id, Self::index_addr(&self.0.table_addrs, table_index, "table")?)))
    }

    /// Get a tag export by name.
    pub fn tag(&self, name: &str) -> Result<Tag> {
        match self.require_export(name)? {
            ExternVal::Tag(tag_addr) => Ok(Tag(StoreItem::new(self.0.store_id, tag_addr))),
            _ => cold!(Err(Error::Other(format!("Export is not a tag: {name}")))),
        }
    }

    /// Get a tag by its module-local index.
    #[cfg_attr(docsrs, doc(cfg(feature = "guest-debug")))]
    #[cfg(feature = "guest-debug")]
    pub fn tag_by_index(&self, tag_index: TagAddr) -> Result<Tag> {
        Ok(Tag(StoreItem::new(self.0.store_id, Self::index_addr(&self.0.tag_addrs, tag_index, "tag")?)))
    }

    /// Get the value of a global export by name.
    pub fn global_get(&self, store: &mut Store, name: &str) -> Result<WasmValue> {
        self.global(name)?.get(store)
    }

    /// Get a global export by name.
    pub fn global(&self, name: &str) -> Result<Global> {
        match self.require_export(name)? {
            ExternVal::Global(global_addr) => Ok(Global(StoreItem::new(self.0.store_id, global_addr))),
            _ => cold!(Err(Error::Other(format!("Export is not a global: {name}")))),
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
        Ok(Global(StoreItem::new(self.0.store_id, Self::index_addr(&self.0.global_addrs, global_index, "global")?)))
    }

    /// Get the start function of the module
    ///
    /// Returns `None` if the module has no start section. Exported functions named
    /// `_start` are not treated as the module's start function.
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{ModuleInstance, Store};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (func $start)
    /// #       (start $start)
    /// #     )
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// # let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate_no_start(&mut store, &module, None)?;
    /// assert!(instance.start_func(&store)?.is_some());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#start-function>
    pub fn start_func(&self, store: &Store) -> Result<Option<Function>> {
        self.validate_store(store)?;

        let Some(func_addr) = self.0.func_start else {
            return Ok(None);
        };

        let func_addr = self.resolve_func_addr(func_addr);
        Ok(Some(Function { item: StoreItem::new(self.0.store_id, func_addr), module_id: self.id() }))
    }

    /// Invoke the start function of the module
    ///
    /// Returns `None` if the module has no start function
    ///
    /// ## Example
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// # use tinywasm::{ModuleInstance, Store};
    /// # let wasm = wat::parse_str(r#"
    /// #     (module
    /// #       (global $g (mut i32) (i32.const 0))
    /// #       (func $start
    /// #         i32.const 7
    /// #         global.set $g)
    /// #       (start $start)
    /// #       (export "g" (global $g)))
    /// # "#).expect("valid wat");
    /// # let module = tinywasm::parse_bytes(&wasm)?;
    /// let mut store = Store::default();
    /// let instance = ModuleInstance::instantiate_no_start(&mut store, &module, None)?;
    ///
    /// assert_eq!(instance.global_get(&mut store, "g")?, 0.into());
    /// assert_eq!(instance.start(&mut store)?, Some(()));
    /// assert_eq!(instance.global_get(&mut store, "g")?, 7.into());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// See <https://webassembly.github.io/spec/core/syntax/modules.html#syntax-start>
    pub fn start(&self, store: &mut Store) -> Result<Option<()>> {
        match self.start_func(store)? {
            Some(func) => func.call(store, &[], &mut []).map(|_| Some(())),
            None => Ok(None),
        }
    }
}
