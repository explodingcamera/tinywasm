use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::{Function, Global, LinkingError, Memory, Result, Table, log};
use tinywasm_types::*;

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[non_exhaustive]
/// An external import value.
pub enum Extern {
    /// A global instance.
    Global(Global),
    /// A table instance.
    Table(Table),
    /// A memory instance.
    Memory(Memory),
    /// A function import.
    Function(Function),
}

impl From<Global> for Extern {
    fn from(value: Global) -> Self {
        Self::Global(value)
    }
}

impl From<Table> for Extern {
    fn from(value: Table) -> Self {
        Self::Table(value)
    }
}

impl From<Memory> for Extern {
    fn from(value: Memory) -> Self {
        Self::Memory(value)
    }
}

impl From<Function> for Extern {
    fn from(value: Function) -> Self {
        Self::Function(value)
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

/// Imports for a module instance
///
/// This is used to link a module instance to its imports
///
/// ## Example
/// ```rust
/// # use log;
/// # fn main() -> tinywasm::Result<()> {
/// use tinywasm::{Global, HostFunction, Imports, Memory, ModuleInstance, Store, Table};
/// use tinywasm::types::{WasmType, TableType, MemoryType, WasmValue};
/// # let wasm = wat::parse_str("(module)").expect("valid wat");
/// # let module = tinywasm::parse_bytes(&wasm)?;
/// # let mut store = Store::default();
/// # let my_other_instance = ModuleInstance::instantiate(&mut store, &module, None)?;
/// let mut imports = Imports::new();
///
/// // function args can be either a single
/// // value that implements `TryFrom<WasmValue>` or a tuple of them
/// let print_i32 = HostFunction::from(&mut store, |_ctx: tinywasm::FuncContext<'_>, arg: i32| {
///     log::debug!("print_i32: {}", arg);
///     Ok(())
/// });
///
/// let table = Table::new(&mut store, TableType::new(WasmType::RefFunc, 10, Some(20)), WasmValue::default_for(WasmType::RefFunc))?;
/// let memory = Memory::new(&mut store, MemoryType::default().with_page_count_initial(1).with_page_count_max(Some(2)))?;
/// let global_i32 = Global::new(&mut store, tinywasm::types::GlobalType::default().with_ty(WasmType::I32), WasmValue::I32(666))?;
///
/// imports
///     .define("my_module", "print_i32", print_i32)
///     .define("my_module", "table", table)
///     .define("my_module", "memory", memory)
///     .define("my_module", "global_i32", global_i32)
///     .link_module("my_other_module", my_other_instance)?;
/// # Ok(())
/// # }
/// ```
/// Now, the imports object can be passed to [`crate::ModuleInstance::instantiate`].
#[derive(Default, Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Imports {
    externs: BTreeMap<ExternName, Extern>,
    modules: BTreeMap<String, crate::ModuleInstance>,
}

pub(crate) struct ResolvedImports {
    pub(crate) globals: Vec<GlobalAddr>,
    pub(crate) tables: Vec<TableAddr>,
    pub(crate) memories: Vec<MemAddr>,
    pub(crate) funcs: Vec<FuncAddr>,
}

impl Imports {
    /// Create a new empty import set
    pub const fn new() -> Self {
        Self { externs: BTreeMap::new(), modules: BTreeMap::new() }
    }

    /// Merge two import sets
    pub fn merge(mut self, other: Self) -> Self {
        self.externs.extend(other.externs);
        self.modules.extend(other.modules);
        self
    }

    /// Link a module
    ///
    /// This will automatically link all imported values on instantiation
    pub fn link_module(&mut self, name: &str, instance: crate::ModuleInstance) -> Result<&mut Self> {
        self.modules.insert(name.to_string(), instance);
        Ok(self)
    }

    /// Define an import value.
    pub fn define(&mut self, module: &str, name: &str, value: impl Into<Extern>) -> &mut Self {
        let name = ExternName { module: module.to_string(), name: name.to_string() };
        self.externs.insert(name, value.into());
        self
    }

    pub(crate) fn take_defined(&self, import: &Import) -> Option<Extern> {
        let name = ExternName::from(import);
        self.externs.get(&name).cloned()
    }

    #[cfg(not(feature = "debug"))]
    fn compare_types<T: PartialEq>(import: &Import, actual: &T, expected: &T) -> Result<()> {
        if expected != actual {
            log::error!("failed to link import {}", import.name);
            return Err(LinkingError::incompatible_import_type(import).into());
        }
        Ok(())
    }

    #[cfg(feature = "debug")]
    fn compare_types<T: PartialEq + Debug>(import: &Import, actual: &T, expected: &T) -> Result<()> {
        if expected != actual {
            log::error!("failed to link import {}: expected {:?}, got {:?}", import.name, expected, actual);
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
            (None, Some(_)) => Err(LinkingError::incompatible_import_type(import).into()),
            (Some(expected_max), Some(actual_max)) if actual_max < expected_max => {
                Err(LinkingError::incompatible_import_type(import).into())
            }
            _ => Ok(()),
        }
    }

    fn compare_memory_types(
        import: &Import,
        expected: &MemoryType,
        actual: &MemoryType,
        real_size: usize,
    ) -> Result<()> {
        Self::compare_types(import, &expected.arch(), &actual.arch())?;

        if actual.page_count_initial() > expected.page_count_initial() && actual.page_count_initial() > real_size as u64
        {
            return Err(LinkingError::incompatible_import_type(import).into());
        }

        if expected.page_size() != actual.page_size() {
            return Err(LinkingError::incompatible_import_type(import).into());
        }

        if expected.page_count_max() > actual.page_count_max() {
            return Err(LinkingError::incompatible_import_type(import).into());
        }

        Ok(())
    }

    pub(crate) fn link(&self, store: &mut crate::Store, module: &Module) -> Result<ResolvedImports> {
        let (global_count, table_count, mem_count, func_count) =
            module.imports.iter().fold((0, 0, 0, 0), |(g, t, m, f), import| match import.kind {
                ImportKind::Global(_) => (g + 1, t, m, f),
                ImportKind::Table(_) => (g, t + 1, m, f),
                ImportKind::Memory(_) => (g, t, m + 1, f),
                ImportKind::Function(_) => (g, t, m, f + 1),
            });

        let mut imports = ResolvedImports {
            globals: Vec::with_capacity(global_count),
            tables: Vec::with_capacity(table_count),
            memories: Vec::with_capacity(mem_count),
            funcs: Vec::with_capacity(func_count),
        };

        for import in &*module.imports {
            if let Some(defined) = self.take_defined(import) {
                match defined {
                    Extern::Global(global) => {
                        let ImportKind::Global(import_ty) = &import.kind else {
                            return Err(LinkingError::incompatible_import_type(import).into());
                        };
                        let global_instance = store.state.get_global(global.0.addr);
                        Self::compare_types(import, &global_instance.ty, import_ty)?;
                        imports.globals.push(global.0.addr);
                    }
                    Extern::Table(table) => {
                        let ImportKind::Table(import_ty) = &import.kind else {
                            return Err(LinkingError::incompatible_import_type(import).into());
                        };
                        let table_instance = store.state.get_table(table.0.addr);
                        let mut kind = table_instance.kind.clone();
                        kind.size_initial = table_instance.size() as u32;
                        Self::compare_table_types(import, &kind, import_ty)?;
                        imports.tables.push(table.0.addr);
                    }
                    Extern::Memory(memory) => {
                        let ImportKind::Memory(import_ty) = &import.kind else {
                            return Err(LinkingError::incompatible_import_type(import).into());
                        };
                        let mem = store.state.get_mem(memory.0.addr);
                        Self::compare_memory_types(import, &mem.kind, import_ty, mem.page_count)?;
                        imports.memories.push(memory.0.addr);
                    }
                    Extern::Function(func_handle) => {
                        let ImportKind::Function(ty) = &import.kind else {
                            return Err(LinkingError::incompatible_import_type(import).into());
                        };
                        let import_func_type = module
                            .func_types
                            .get(*ty as usize)
                            .ok_or_else(|| LinkingError::incompatible_import_type(import))?;
                        func_handle.item.validate_store(store)?;
                        Self::compare_types(import, &func_handle.ty, import_func_type)?;
                        imports.funcs.push(func_handle.addr);
                    }
                }
                continue;
            }

            let name = ExternName::from(import);
            let Some(instance) = self.modules.get(&name.module) else {
                return Err(LinkingError::unknown_import(import).into());
            };
            instance.validate_store(store)?;

            let val = instance.export_addr(&import.name).ok_or_else(|| LinkingError::unknown_import(import))?;

            {
                // check if the kind matches
                if val.kind() != (&import.kind).into() {
                    return Err(LinkingError::incompatible_import_type(import).into());
                }

                match (val, &import.kind) {
                    (ExternVal::Global(global_addr), ImportKind::Global(ty)) => {
                        let global = store.state.get_global(global_addr);
                        Self::compare_types(import, &global.ty, ty)?;
                        imports.globals.push(global_addr);
                    }
                    (ExternVal::Table(table_addr), ImportKind::Table(ty)) => {
                        let table = store.state.get_table(table_addr);
                        let mut kind = table.kind.clone();
                        kind.size_initial = table.size() as u32;
                        Self::compare_table_types(import, &kind, ty)?;
                        imports.tables.push(table_addr);
                    }
                    (ExternVal::Memory(memory_addr), ImportKind::Memory(ty)) => {
                        let mem = store.state.get_mem(memory_addr);
                        Self::compare_memory_types(import, &mem.kind, ty, mem.page_count)?;
                        imports.memories.push(memory_addr);
                    }
                    (ExternVal::Func(func_addr), ImportKind::Function(ty)) => {
                        let func = store.state.get_func(func_addr);
                        let import_func_type = module
                            .func_types
                            .get(*ty as usize)
                            .ok_or_else(|| LinkingError::incompatible_import_type(import))?;

                        Self::compare_types(import, func.ty(), import_func_type)?;
                        imports.funcs.push(func_addr);
                    }
                    _ => return Err(LinkingError::incompatible_import_type(import).into()),
                }
            }
        }

        Ok(imports)
    }
}
