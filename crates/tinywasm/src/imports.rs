#![allow(dead_code)]

use core::fmt::Debug;

use crate::{
    func::{FromWasmValueTuple, IntoWasmValueTuple, ValTypesFromTuple},
    Result,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use tinywasm_types::*;

/// The internal representation of a function
#[derive(Debug, Clone)]
pub enum Function {
    /// A host function
    Host(HostFunction),

    /// A function defined in WebAssembly
    Wasm(WasmFunction),
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
#[derive(Clone)]
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

pub(crate) type HostFuncInner =
    Arc<dyn Fn(FuncContext<'_>, &[WasmValue]) -> Result<Vec<WasmValue>> + 'static + Send + Sync>;

/// The context of a host-function call
#[derive(Debug)]
pub struct FuncContext<'a> {
    pub(crate) store: &'a mut crate::Store,
    pub(crate) module: &'a crate::ModuleInstance,
}

impl FuncContext<'_> {
    /// Get a mutable reference to the store
    pub fn store_mut(&mut self) -> &mut crate::Store {
        self.store
    }

    /// Get a reference to the store
    pub fn store(&self) -> &crate::Store {
        self.store
    }

    /// Get a reference to the module instance
    pub fn module(&self) -> &crate::ModuleInstance {
        self.module
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
    Global(ExternGlobal),

    /// A table
    Table(ExternTable),

    /// A memory
    Memory(ExternMemory),

    /// A function
    Function(Function),
}

/// A function
#[derive(Debug, Clone)]
pub struct ExternFunc(pub(crate) HostFunction);

/// A global value
#[derive(Debug, Clone)]
pub struct ExternGlobal {
    pub(crate) ty: GlobalType,
    pub(crate) val: WasmValue,
}

/// A table
#[derive(Debug, Clone)]
pub struct ExternTable {
    pub(crate) ty: TableType,
    pub(crate) val: WasmValue,
}

/// A memory
#[derive(Debug, Clone)]
pub struct ExternMemory {
    pub(crate) ty: MemoryType,
}

impl Extern {
    /// Create a new global import
    pub fn global(val: WasmValue, mutable: bool) -> Self {
        Self::Global(ExternGlobal { ty: GlobalType { ty: val.val_type(), mutable }, val })
    }

    /// Create a new table import
    pub fn table(ty: TableType, val: WasmValue) -> Self {
        Self::Table(ExternTable { ty, val })
    }

    /// Create a new memory import
    pub fn memory(ty: MemoryType) -> Self {
        Self::Memory(ExternMemory { ty })
    }

    /// Create a new function import
    pub fn func(
        ty: &tinywasm_types::FuncType,
        func: impl Fn(FuncContext<'_>, &[WasmValue]) -> Result<Vec<WasmValue>> + 'static + Send + Sync,
    ) -> Self {
        let inner_func = move |ctx: FuncContext<'_>, args: &[WasmValue]| {
            let args = args.to_vec();
            func(ctx, &args)
        };

        Self::Function(Function::Host(HostFunction { func: Arc::new(inner_func), ty: ty.clone() }))
    }

    /// Create a new typed function import
    pub fn typed_func<P, R>(func: impl Fn(FuncContext<'_>, P) -> Result<R> + 'static + Send + Sync) -> Self
    where
        P: FromWasmValueTuple + ValTypesFromTuple,
        R: IntoWasmValueTuple + ValTypesFromTuple + Debug,
    {
        let inner_func = move |ctx: FuncContext<'_>, args: &[WasmValue]| -> Result<Vec<WasmValue>> {
            log::error!("args: {:?}", args);
            let args = P::from_wasm_value_tuple(args.to_vec())?;
            let result = func(ctx, args)?;
            log::error!("result: {:?}", result);
            Ok(result.into_wasm_value_tuple())
        };

        let ty = tinywasm_types::FuncType { params: P::val_types(), results: R::val_types() };

        Self::Function(Function::Host(HostFunction { func: Arc::new(inner_func), ty }))
    }

    pub(crate) fn kind(&self) -> ExternalKind {
        match self {
            Self::Global(_) => ExternalKind::Global,
            Self::Table(_) => ExternalKind::Table,
            Self::Memory(_) => ExternalKind::Memory,
            Self::Function(_) => ExternalKind::Func,
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
pub struct Imports {
    values: BTreeMap<ExternName, Extern>,
    modules: BTreeMap<String, ModuleInstanceAddr>,
}

pub(crate) enum ResolvedExtern<S, V> {
    // already in the store
    Store(S),

    // needs to be added to the store, provided value
    Extern(V),
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
            let export_addr = instance.export(&import.name)?;

            // TODO: validate kind and type
            match &export_addr {
                ExternVal::Global(_) => {}
                ExternVal::Table(_) => {}
                ExternVal::Mem(_) => {}
                ExternVal::Func(_) => {}
            }

            return Some(ResolvedExtern::Store(export_addr));
        }

        None
    }

    fn compare_types<T>(import: &Import, expected: &T, actual: &T) -> Result<()>
    where
        T: Debug + PartialEq,
    {
        if expected != actual {
            log::error!("failed to link import {}, expected {:?}, got {:?}", import.name, expected, actual);
            return Err(crate::LinkingError::IncompatibleImportType {
                module: import.module.to_string(),
                name: import.name.to_string(),
            }
            .into());
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

        for import in module.data.imports.iter() {
            let Some(val) = self.take(store, import) else {
                return Err(crate::LinkingError::UnknownImport {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                }
                .into());
            };

            match val {
                // A link to something that needs to be added to the store
                ResolvedExtern::Extern(ex) => match (ex, &import.kind) {
                    (Extern::Global(extern_global), ImportKind::Global(ty)) => {
                        Self::compare_types(import, &extern_global.ty, ty)?;
                        imports.globals.push(store.add_global(extern_global.ty, extern_global.val.into(), idx)?);
                    }
                    (Extern::Table(extern_table), ImportKind::Table(ty)) => {
                        Self::compare_types(import, &extern_table.ty.element_type, &ty.element_type)?;
                        // TODO: do we need to check any limits?
                        imports.tables.push(store.add_table(extern_table.ty, idx)?);
                    }
                    (Extern::Memory(extern_memory), ImportKind::Memory(ty)) => {
                        Self::compare_types(import, &extern_memory.ty.arch, &ty.arch)?;
                        // TODO: do we need to check any limits?
                        imports.memories.push(store.add_mem(extern_memory.ty, idx)?);
                    }
                    (Extern::Function(extern_func), ImportKind::Function(ty)) => {
                        let import_func_type = module.data.func_types.get(*ty as usize).ok_or_else(|| {
                            crate::LinkingError::IncompatibleImportType {
                                module: import.module.to_string(),
                                name: import.name.to_string(),
                            }
                        })?;

                        Self::compare_types(import, extern_func.ty(), import_func_type)?;
                        imports.funcs.push(store.add_func(extern_func, *ty, idx)?);
                    }
                    _ => {
                        return Err(crate::LinkingError::IncompatibleImportType {
                            module: import.module.to_string(),
                            name: import.name.to_string(),
                        }
                        .into());
                    }
                },

                // A link to something already in the store
                ResolvedExtern::Store(val) => {
                    // check if the kind matches
                    if val.kind() != (&import.kind).into() {
                        return Err(crate::LinkingError::IncompatibleImportType {
                            module: import.module.to_string(),
                            name: import.name.to_string(),
                        }
                        .into());
                    }

                    // TODO: check if the type matches

                    match val {
                        ExternVal::Global(g) => imports.globals.push(g),
                        ExternVal::Table(t) => imports.tables.push(t),
                        ExternVal::Mem(m) => imports.memories.push(m),
                        ExternVal::Func(f) => imports.funcs.push(f),
                    }
                }
            }
        }

        Ok(imports)
    }
}
