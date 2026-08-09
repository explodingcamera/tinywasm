use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::hint::cold_path;

use crate::{Function, Global, HostFunction, LinkingError, Memory, Result, Table};
use tinywasm_types::*;

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(core::fmt::Debug))]
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
    /// A reusable host function definition.
    HostFunction(HostFunction),
}

macro_rules! impl_conv {
    ($($ty:ty => $variant:ident),* $(,)?) => {
        $(
            impl From<$ty> for Extern {
                fn from(value: $ty) -> Self {
                    Self::$variant(value)
                }
            }
        )*
    };
}

impl_conv! {
    Global => Global,
    Table => Table,
    Memory => Memory,
    Function => Function,
    HostFunction => HostFunction,
}

/// Imports for a module instance
///
/// This is used to link a module instance to its imports
///
/// ## Example
/// ```rust
/// # use log;
/// # fn main() -> tinywasm::Result<()> {
/// use tinywasm::types::{GlobalType, MemoryType, TableType, WasmType, WasmValue};
/// use tinywasm::{Global, HostFunction, Imports, Memory, ModuleInstance, Store, Table};
/// # let wasm = wat::parse_str("(module)").expect("valid wat");
/// # let module = tinywasm::parse_bytes(&wasm)?;
/// # let mut store = Store::default();
/// # let my_other_instance = ModuleInstance::instantiate(&mut store, &module, None)?;
/// let mut imports = Imports::new();
///
/// let print_i32 = HostFunction::from(|_ctx: tinywasm::FuncContext<'_>, arg: i32| {
///     log::debug!("print_i32: {}", arg);
///     Ok(())
/// });
///
/// let table = Table::new(
///     &mut store,
///     TableType::new(tinywasm::types::RefType::FUNCREF, 10, Some(20)),
///     tinywasm::types::RefValue::Null.into(),
/// )?;
/// let memory = Memory::new(
///     &mut store,
///     MemoryType::default().with_page_count_initial(1).with_page_count_max(Some(2)),
/// )?;
/// let global_i32 =
///     Global::new(&mut store, GlobalType::default().with_ty(WasmType::I32), WasmValue::I32(666))?;
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
/// Host function definitions are store-independent, so the imports object can be borrowed by
/// [`crate::ModuleInstance::instantiate`] for multiple stores.
/// TinyWasm also matches GC reference types to each module when it links an imported host function.
#[derive(Default, Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Imports {
    externs: BTreeMap<String, BTreeMap<String, Extern>>,
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
        for (module, externs) in other.externs {
            self.externs.entry(module).or_default().extend(externs);
        }
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
    ///
    /// A [`Function`], [`Global`], [`Table`], or [`Memory`] handle belongs to
    /// one store and can only be imported into that store. A [`HostFunction`]
    /// is a reusable definition and can be imported into multiple stores.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # fn main() -> tinywasm::Result<()> {
    /// use tinywasm::{HostFunction, Imports, ModuleInstance, Store};
    ///
    /// let wasm = wat::parse_str(
    ///     r#"
    ///     (module
    ///       (import "host" "answer" (func $answer (result i32)))
    ///       (export "answer" (func $answer)))
    /// "#,
    /// )
    /// .expect("valid wat");
    /// let module = tinywasm::parse_bytes(&wasm)?;
    /// let mut imports = Imports::new();
    /// imports.define("host", "answer", HostFunction::from(|_ctx, ()| Ok(42_i32)));
    ///
    /// let mut first_store = Store::default();
    /// let first = ModuleInstance::instantiate(&mut first_store, &module, Some(&imports))?;
    /// assert_eq!(first.func::<(), i32>(&first_store, "answer")?.call(&mut first_store, ())?, 42);
    ///
    /// let mut second_store = Store::default();
    /// let second = ModuleInstance::instantiate(&mut second_store, &module, Some(&imports))?;
    /// assert_eq!(second.func::<(), i32>(&second_store, "answer")?.call(&mut second_store, ())?, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub fn define(&mut self, module: &str, name: &str, value: impl Into<Extern>) -> &mut Self {
        self.externs.entry(module.to_string()).or_default().insert(name.to_string(), value.into());
        self
    }

    pub(crate) fn take_defined(&self, import: &Import) -> Option<Extern> {
        self.externs.get(import.module.as_ref())?.get(import.name.as_ref()).cloned()
    }

    fn compare_types<T: PartialEq>(import: &Import, actual: &T, expected: &T) -> Result<()> {
        if expected != actual {
            cold_path();
            return Err(LinkingError::incompatible_import_type(import).into());
        }
        Ok(())
    }

    fn compare_table_types(import: &Import, actual: &TableType, expected: &TableType) -> Result<()> {
        Self::compare_types(import, &actual.arch(), &expected.arch())?;
        if actual.element_type != expected.element_type {
            return Err(LinkingError::incompatible_import_type(import).into());
        }
        if actual.size_initial < expected.size_initial {
            cold_path();
            return Err(LinkingError::incompatible_import_type(import).into());
        }

        match expected.size_max {
            Some(expected_max) if actual.size_max.is_none_or(|actual_max| actual_max > expected_max) => {
                cold_path();
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

    pub(crate) fn link(
        &self,
        store: &mut crate::Store,
        module: &Module,
        type_addrs: &[TypeAddr],
    ) -> Result<ResolvedImports> {
        let (global_count, table_count, mem_count, func_count) =
            module.imports.iter().fold((0, 0, 0, 0), |(g, t, m, f), import| match import.kind {
                ImportKind::Global(_) => (g + 1, t, m, f),
                ImportKind::Table(_) => (g, t + 1, m, f),
                ImportKind::Memory(_) => (g, t, m + 1, f),
                ImportKind::Function(_) => (g, t, m, f + 1),
            });

        let mut imports = ResolvedImports {
            globals: Vec::with_capacity(global_count + module.globals.len()),
            tables: Vec::with_capacity(table_count + module.tables.len()),
            memories: Vec::with_capacity(mem_count + module.memory_types.len()),
            funcs: Vec::with_capacity(func_count + module.funcs.len()),
        };

        for import in &*module.imports {
            let (val, func_handle) = if let Some(defined) = self.take_defined(import) {
                match defined {
                    Extern::Global(global) => {
                        global.0.validate_store(store)?;
                        (ExternVal::Global(global.0.addr), None)
                    }
                    Extern::Table(table) => {
                        table.0.validate_store(store)?;
                        (ExternVal::Table(table.0.addr), None)
                    }
                    Extern::Memory(memory) => {
                        memory.0.validate_store(store)?;
                        (ExternVal::Memory(memory.0.addr), None)
                    }
                    Extern::Function(func) => (ExternVal::Func(func.addr()), Some(func)),
                    Extern::HostFunction(func) => {
                        (ExternVal::Func(func.instantiate_for_import(store, type_addrs)?.addr()), None)
                    }
                }
            } else {
                let Some(instance) = self.modules.get(import.module.as_ref()) else {
                    cold_path();
                    return Err(LinkingError::unknown_import(import).into());
                };
                instance.validate_store(store)?;
                (instance.export_addr(&import.name).ok_or_else(|| LinkingError::unknown_import(import))?, None)
            };

            if val.kind() != (&import.kind).into() {
                cold_path();
                return Err(LinkingError::incompatible_import_type(import).into());
            }

            match (val, &import.kind) {
                (ExternVal::Global(global_addr), ImportKind::Global(ty)) => {
                    let global_ty = store.state.globals.ty(global_addr);
                    let expected = ty.with_ty(crate::store::canonicalize_value_type(ty.ty, type_addrs));
                    let compatible = global_ty.mutable == ty.mutable
                        && store.state.value_type_is_subtype(global_ty.ty, expected.ty)
                        && (!ty.mutable || store.state.value_type_is_subtype(expected.ty, global_ty.ty));
                    if !compatible {
                        cold_path();
                        return Err(LinkingError::incompatible_import_type(import).into());
                    }
                    imports.globals.push(global_addr);
                }
                (ExternVal::Table(table_addr), ImportKind::Table(ty)) => {
                    let table = store.state.get_table(table_addr);
                    let mut kind = table.kind;
                    kind.size_initial = table.size() as u64;
                    let element_type = crate::store::canonicalize_ref_type(ty.element_type, type_addrs);
                    let expected = match ty.arch() {
                        MemoryArch::I32 => TableType::new(element_type, ty.size_initial, ty.size_max),
                        MemoryArch::I64 => TableType::new64(element_type, ty.size_initial, ty.size_max),
                    };
                    Self::compare_table_types(import, &kind, &expected)?;
                    imports.tables.push(table_addr);
                }
                (ExternVal::Memory(memory_addr), ImportKind::Memory(ty)) => {
                    let mem = store.state.get_mem(memory_addr);
                    Self::compare_memory_types(import, &mem.kind, ty, mem.page_count)?;
                    imports.memories.push(memory_addr);
                }
                (ExternVal::Func(func_addr), ImportKind::Function(ty)) => {
                    let expected_type_addr =
                        type_addrs.get(*ty as usize).ok_or_else(|| LinkingError::incompatible_import_type(import))?;
                    if let Some(func) = &func_handle {
                        func.item.validate_store(store)?;
                    }
                    if !store.state.type_addr_is_subtype(store.state.get_func(func_addr).type_addr, *expected_type_addr)
                    {
                        cold_path();
                        return Err(LinkingError::incompatible_import_type(import).into());
                    }
                    imports.funcs.push(func_addr);
                }
                _ => unreachable!("import kind checked above"),
            }
        }

        Ok(imports)
    }
}
