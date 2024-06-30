use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::func::{FromWasmValueTuple, IntoWasmValueTuple, ValTypesFromTuple};
use crate::{log, LinkingError, MemoryRef, MemoryRefMut, Result};
use tinywasm_types::*;

/// The internal representation of a function
#[derive(Debug, Clone)]
pub enum Function {
    /// A host function
    Host(Rc<HostFunction>),

    /// A pointer to a WebAssembly function
    Wasm(Rc<WasmFunction>),
}

impl Function {
    pub(crate) fn ty(&self) -> &FuncType {
        match self {
            Self::Host(f) => &f.ty,
            Self::Wasm(f) => &f.ty,
        }
    }
}

/// A host function
pub struct HostFunction {
    pub(crate) ty: tinywasm_types::FuncType,
    pub(crate) func: HostFuncInner,
}

impl HostFunction {
    /// Get the function's type
    pub fn ty(&self) -> &tinywasm_types::FuncType {
        &self.ty
    }

    /// Call the function
    pub fn call(&self, ctx: FuncContext<'_>, args: &[WasmValue]) -> Result<Vec<WasmValue>> {
        (self.func)(ctx, args)
    }
}

pub(crate) type HostFuncInner = Box<dyn Fn(FuncContext<'_>, &[WasmValue]) -> Result<Vec<WasmValue>>>;

/// The context of a host-function call
#[derive(Debug)]
pub struct FuncContext<'a> {
    pub(crate) store: &'a mut crate::Store,
    pub(crate) module_addr: ModuleInstanceAddr,
}

impl FuncContext<'_> {
    /// Get a reference to the store
    pub fn store(&self) -> &crate::Store {
        self.store
    }

    /// Get a mutable reference to the store
    pub fn store_mut(&mut self) -> &mut crate::Store {
        self.store
    }

    /// Get a reference to the module instance
    pub fn module(&self) -> crate::ModuleInstance {
        self.store.get_module_instance_raw(self.module_addr)
    }

    /// Get a reference to an exported memory
    pub fn exported_memory(&mut self, name: &str) -> Result<MemoryRef<'_>> {
        self.module().exported_memory(self.store, name)
    }

    /// Get a reference to an exported memory
    pub fn exported_memory_mut(&mut self, name: &str) -> Result<MemoryRefMut<'_>> {
        self.module().exported_memory_mut(self.store, name)
    }
}

impl Debug for HostFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HostFunction").field("ty", &self.ty).field("func", &"...").finish()
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
/// An external value
pub enum Extern {
    /// A global value
    Global {
        /// The type of the global value.
        ty: GlobalType,
        /// The actual value of the global, encapsulated in `WasmValue`.
        val: WasmValue,
    },

    /// A table
    Table {
        /// Defines the type of the table, including its element type and limits.
        ty: TableType,
        /// The initial value of the table.
        init: WasmValue,
    },

    /// A memory
    Memory {
        /// Defines the type of the memory, including its limits and the type of its pages.
        ty: MemoryType,
    },

    /// A function
    Function(Function),
}

impl Extern {
    /// Create a new global import
    pub fn global(val: WasmValue, mutable: bool) -> Self {
        Self::Global { ty: GlobalType { ty: val.val_type(), mutable }, val }
    }

    /// Create a new table import
    pub fn table(ty: TableType, init: WasmValue) -> Self {
        Self::Table { ty, init }
    }

    /// Create a new memory import
    pub fn memory(ty: MemoryType) -> Self {
        Self::Memory { ty }
    }

    /// Create a new function import
    pub fn func(
        ty: &tinywasm_types::FuncType,
        func: impl Fn(FuncContext<'_>, &[WasmValue]) -> Result<Vec<WasmValue>> + 'static,
    ) -> Self {
        Self::Function(Function::Host(Rc::new(HostFunction { func: Box::new(func), ty: ty.clone() })))
    }

    /// Create a new typed function import
    // TODO: currently, this is slower than `Extern::func` because of the type conversions.
    //       we should be able to optimize this and make it even faster than `Extern::func`.
    pub fn typed_func<P, R>(func: impl Fn(FuncContext<'_>, P) -> Result<R> + 'static) -> Self
    where
        P: FromWasmValueTuple + ValTypesFromTuple,
        R: IntoWasmValueTuple + ValTypesFromTuple + Debug,
    {
        let inner_func = move |ctx: FuncContext<'_>, args: &[WasmValue]| -> Result<Vec<WasmValue>> {
            let args = P::from_wasm_value_tuple(args)?;
            let result = func(ctx, args)?;
            Ok(result.into_wasm_value_tuple().to_vec())
        };

        let ty = tinywasm_types::FuncType { params: P::val_types(), results: R::val_types() };
        Self::Function(Function::Host(Rc::new(HostFunction { func: Box::new(inner_func), ty })))
    }

    /// Get the kind of the external value
    pub fn kind(&self) -> ExternalKind {
        match self {
            Self::Global { .. } => ExternalKind::Global,
            Self::Table { .. } => ExternalKind::Table,
            Self::Memory { .. } => ExternalKind::Memory,
            Self::Function { .. } => ExternalKind::Func,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
/// Name of an import
pub struct ExternName {
    module: String,
    name: String,
}

impl From<&Import> for ExternName {
    fn from(import: &Import) -> Self {
        Self { module: import.module.to_string(), name: import.name.to_string() }
    }
}

#[derive(Debug, Default)]
/// Imports for a module instance
///
/// This is used to link a module instance to its imports
///
/// ## Example
/// ```rust
/// # use log;
/// # fn main() -> tinywasm::Result<()> {
/// use tinywasm::{Imports, Extern};
/// use tinywasm::types::{ValType, TableType, MemoryType, WasmValue};
/// let mut imports = Imports::new();
///
/// // function args can be either a single
/// // value that implements `TryFrom<WasmValue>` or a tuple of them
/// let print_i32 = Extern::typed_func(|_ctx: tinywasm::FuncContext<'_>, arg: i32| {
///     log::debug!("print_i32: {}", arg);
///     Ok(())
/// });
///
/// let table_type = TableType::new(ValType::RefFunc, 10, Some(20));
/// let table_init = WasmValue::default_for(ValType::RefFunc);
///
/// imports
///     .define("my_module", "print_i32", print_i32)?
///     .define("my_module", "table", Extern::table(table_type, table_init))?
///     .define("my_module", "memory", Extern::memory(MemoryType::new_32(1, Some(2))))?
///     .define("my_module", "global_i32", Extern::global(WasmValue::I32(666), false))?
///     .link_module("my_other_module", 0)?;
/// # Ok(())
/// # }
/// ```
///
/// Note that module instance addresses for [`Imports::link_module`] can be obtained from [`crate::ModuleInstance::id`].
/// Now, the imports object can be passed to [`crate::ModuleInstance::instantiate`].
#[derive(Clone)]
pub struct Imports {
    values: BTreeMap<ExternName, Extern>,
    modules: BTreeMap<String, ModuleInstanceAddr>,
}

pub(crate) enum ResolvedExtern<S, V> {
    Store(S),  // already in the store
    Extern(V), // needs to be added to the store, provided value
}

pub(crate) struct ResolvedImports {
    pub(crate) globals: Vec<GlobalAddr>,
    pub(crate) tables: Vec<TableAddr>,
    pub(crate) memories: Vec<MemAddr>,
    pub(crate) funcs: Vec<FuncAddr>,
}

impl ResolvedImports {
    pub(crate) fn new() -> Self {
        Self { globals: Vec::new(), tables: Vec::new(), memories: Vec::new(), funcs: Vec::new() }
    }
}

impl Imports {
    /// Create a new empty import set
    pub fn new() -> Self {
        Imports { values: BTreeMap::new(), modules: BTreeMap::new() }
    }

    /// Merge two import sets
    pub fn merge(mut self, other: Self) -> Self {
        self.values.extend(other.values);
        self.modules.extend(other.modules);
        self
    }

    /// Link a module
    ///
    /// This will automatically link all imported values on instantiation
    pub fn link_module(&mut self, name: &str, addr: ModuleInstanceAddr) -> Result<&mut Self> {
        self.modules.insert(name.to_string(), addr);
        Ok(self)
    }

    /// Define an import
    pub fn define(&mut self, module: &str, name: &str, value: Extern) -> Result<&mut Self> {
        self.values.insert(ExternName { module: module.to_string(), name: name.to_string() }, value);
        Ok(self)
    }

    pub(crate) fn take(
        &mut self,
        store: &mut crate::Store,
        import: &Import,
    ) -> Option<ResolvedExtern<ExternVal, Extern>> {
        let name = ExternName::from(import);
        if let Some(v) = self.values.get(&name) {
            return Some(ResolvedExtern::Extern(v.clone()));
        }
        if let Some(addr) = self.modules.get(&name.module) {
            let instance = store.get_module_instance(*addr)?;
            return Some(ResolvedExtern::Store(instance.export_addr(&import.name)?));
        }

        None
    }

    fn compare_types<T: Debug + PartialEq>(import: &Import, actual: &T, expected: &T) -> Result<()> {
        if expected != actual {
            log::error!("failed to link import {}, expected {:?}, got {:?}", import.name, expected, actual);
            return Err(LinkingError::incompatible_import_type(import).into());
        }
        Ok(())
    }

    fn compare_table_types(import: &Import, expected: &TableType, actual: &TableType) -> Result<()> {
        Self::compare_types(import, &actual.element_type, &expected.element_type)?;

        if actual.size_initial > expected.size_initial {
            return Err(LinkingError::incompatible_import_type(import).into());
        }

        match (expected.size_max, actual.size_max) {
            (None, Some(_)) => return Err(LinkingError::incompatible_import_type(import).into()),
            (Some(expected_max), Some(actual_max)) if actual_max < expected_max => {
                return Err(LinkingError::incompatible_import_type(import).into())
            }
            _ => {}
        }

        Ok(())
    }

    fn compare_memory_types(
        import: &Import,
        expected: &MemoryType,
        actual: &MemoryType,
        real_size: Option<usize>,
    ) -> Result<()> {
        Self::compare_types(import, &expected.arch, &actual.arch)?;

        if actual.page_count_initial > expected.page_count_initial
            && real_size.map_or(true, |size| actual.page_count_initial > size as u64)
        {
            return Err(LinkingError::incompatible_import_type(import).into());
        }

        if expected.page_count_max.is_none() && actual.page_count_max.is_some() {
            return Err(LinkingError::incompatible_import_type(import).into());
        }

        if let (Some(expected_max), Some(actual_max)) = (expected.page_count_max, actual.page_count_max) {
            if actual_max < expected_max {
                return Err(LinkingError::incompatible_import_type(import).into());
            }
        }

        Ok(())
    }

    pub(crate) fn link(
        mut self,
        store: &mut crate::Store,
        module: &crate::Module,
        idx: ModuleInstanceAddr,
    ) -> Result<ResolvedImports> {
        let mut imports = ResolvedImports::new();

        for import in module.0.imports.iter() {
            let val = self.take(store, import).ok_or_else(|| LinkingError::unknown_import(import))?;

            match val {
                // A link to something that needs to be added to the store
                ResolvedExtern::Extern(ex) => match (ex, &import.kind) {
                    (Extern::Global { ty, val }, ImportKind::Global(import_ty)) => {
                        Self::compare_types(import, &ty, import_ty)?;
                        imports.globals.push(store.add_global(ty, val.into(), idx)?);
                    }
                    (Extern::Table { ty, .. }, ImportKind::Table(import_ty)) => {
                        Self::compare_table_types(import, &ty, import_ty)?;
                        imports.tables.push(store.add_table(ty, idx)?);
                    }
                    (Extern::Memory { ty }, ImportKind::Memory(import_ty)) => {
                        Self::compare_memory_types(import, &ty, import_ty, None)?;
                        imports.memories.push(store.add_mem(ty, idx)?);
                    }
                    (Extern::Function(extern_func), ImportKind::Function(ty)) => {
                        let import_func_type = module
                            .0
                            .func_types
                            .get(*ty as usize)
                            .ok_or_else(|| LinkingError::incompatible_import_type(import))?;

                        Self::compare_types(import, extern_func.ty(), import_func_type)?;
                        imports.funcs.push(store.add_func(extern_func, idx)?);
                    }
                    _ => return Err(LinkingError::incompatible_import_type(import).into()),
                },

                // A link to something already in the store
                ResolvedExtern::Store(val) => {
                    // check if the kind matches
                    if val.kind() != (&import.kind).into() {
                        return Err(LinkingError::incompatible_import_type(import).into());
                    }

                    match (val, &import.kind) {
                        (ExternVal::Global(global_addr), ImportKind::Global(ty)) => {
                            let global = store.get_global(global_addr)?;
                            Self::compare_types(import, &global.ty, ty)?;
                            imports.globals.push(global_addr);
                        }
                        (ExternVal::Table(table_addr), ImportKind::Table(ty)) => {
                            let table = store.get_table(table_addr)?;
                            Self::compare_table_types(import, &table.kind, ty)?;
                            imports.tables.push(table_addr);
                        }
                        (ExternVal::Memory(memory_addr), ImportKind::Memory(ty)) => {
                            let mem = store.get_mem(memory_addr)?;
                            let (size, kind) = { (mem.page_count(), mem.kind) };
                            Self::compare_memory_types(import, &kind, ty, Some(size))?;
                            imports.memories.push(memory_addr);
                        }
                        (ExternVal::Func(func_addr), ImportKind::Function(ty)) => {
                            let func = store.get_func(func_addr)?;
                            let import_func_type = module
                                .0
                                .func_types
                                .get(*ty as usize)
                                .ok_or_else(|| LinkingError::incompatible_import_type(import))?;

                            Self::compare_types(import, func.func.ty(), import_func_type)?;
                            imports.funcs.push(func_addr);
                        }
                        _ => return Err(LinkingError::incompatible_import_type(import).into()),
                    }
                }
            }
        }

        Ok(imports)
    }
}
