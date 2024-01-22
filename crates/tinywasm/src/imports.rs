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
use tinywasm_types::{
    Addr, Export, ExternVal, ExternalKind, FuncAddr, GlobalAddr, GlobalType, Import, MemAddr, MemoryType,
    ModuleInstanceAddr, TableAddr, TableType, TypeAddr, WasmFunction, WasmValue,
};

/// The internal representation of a function
#[derive(Debug, Clone)]
pub enum Function {
    /// A host function
    Host(HostFunction),

    /// A function defined in WebAssembly
    Wasm(WasmFunction),
}

impl Function {
    /// Get the function's type
    pub fn ty(&self, module: &crate::ModuleInstance) -> tinywasm_types::FuncType {
        match self {
            Self::Host(f) => f.ty.clone(),
            Self::Wasm(f) => module.func_ty(f.ty_addr).clone(),
        }
    }
}

/// A host function
#[derive(Clone)]
pub struct HostFunction {
    pub(crate) ty: tinywasm_types::FuncType,
    pub(crate) func: Arc<dyn Fn(&mut crate::Store, &[WasmValue]) -> Result<Vec<WasmValue>> + 'static + Send + Sync>,
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
    Func(Function),
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
        func: impl Fn(&mut crate::Store, &[WasmValue]) -> Result<Vec<WasmValue>> + 'static + Send + Sync,
    ) -> Self {
        let inner_func = move |store: &mut crate::Store, args: &[WasmValue]| {
            let args = args.to_vec();
            func(store, &args)
        };

        Self::Func(Function::Host(HostFunction { func: Arc::new(inner_func), ty: ty.clone() }))
    }

    /// Create a new typed function import
    pub fn typed_func<P, R>(func: impl Fn(&mut crate::Store, P) -> Result<R> + 'static + Send + Sync) -> Self
    where
        P: FromWasmValueTuple + ValTypesFromTuple,
        R: IntoWasmValueTuple + ValTypesFromTuple,
    {
        let inner_func = move |store: &mut crate::Store, args: &[WasmValue]| -> Result<Vec<WasmValue>> {
            let args = P::from_wasm_value_tuple(args.to_vec())?;
            let result = func(store, args)?;
            Ok(result.into_wasm_value_tuple())
        };

        let ty = tinywasm_types::FuncType { params: P::val_types(), results: R::val_types() };

        Self::Func(Function::Host(HostFunction { func: Arc::new(inner_func), ty }))
    }

    pub(crate) fn kind(&self) -> ExternalKind {
        match self {
            Self::Global(_) => ExternalKind::Global,
            Self::Table(_) => ExternalKind::Table,
            Self::Memory(_) => ExternalKind::Memory,
            Self::Func(_) => ExternalKind::Func,
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
    pub(crate) mems: Vec<MemAddr>,
    pub(crate) funcs: Vec<FuncAddr>,
}

impl ResolvedImports {
    pub(crate) fn new() -> Self {
        Self { globals: Vec::new(), tables: Vec::new(), mems: Vec::new(), funcs: Vec::new() }
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
        log::error!("provided externs: {:?}", self.values.keys());
        if let Some(v) = self.values.get(&name) {
            return Some(ResolvedExtern::Extern(v.clone()));
        }
        log::error!("failed to resolve import: {:?}", name);
        // TODO:
        // if let Some(addr) = self.modules.get(&name.module) {
        //     let instance = store.get_module_instance(*addr)?;
        //     let exports = instance.exports();

        //     let export = exports.get_untyped(&import.name)?;
        //     let addr = match export.kind {
        //         ExternalKind::Global(g) => ExternVal::Global(),
        //     };

        //     return Some(ResolvedExtern::Store());
        // }

        None
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
                return Err(crate::Error::MissingImport {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                });
            };

            match val {
                // A link to something that needs to be added to the store
                ResolvedExtern::Extern(ex) => {
                    // check if the kind matches
                    let kind = ex.kind();
                    if kind != (&import.kind).into() {
                        return Err(crate::Error::InvalidImportType {
                            module: import.module.to_string(),
                            name: import.name.to_string(),
                        });
                    }

                    // TODO: check if the type matches

                    // add it to the store and get the address
                    let addr = match ex {
                        Extern::Global(g) => store.add_global(g.ty, g.val.into(), idx)?,
                        Extern::Table(t) => store.add_table(t.ty, idx)?,
                        Extern::Memory(m) => store.add_mem(m.ty, idx)?,
                        Extern::Func(f) => store.add_func(f, idx)?,
                    };

                    // store the link
                    match &kind {
                        ExternalKind::Global => imports.globals.push(addr),
                        ExternalKind::Table => imports.tables.push(addr),
                        ExternalKind::Memory => imports.mems.push(addr),
                        ExternalKind::Func => imports.funcs.push(addr),
                    }
                }

                // A link to something already in the store
                ResolvedExtern::Store(val) => {
                    // check if the kind matches
                    if val.kind() != (&import.kind).into() {
                        return Err(crate::Error::InvalidImportType {
                            module: import.module.to_string(),
                            name: import.name.to_string(),
                        });
                    }

                    // TODO: check if the type matches

                    match val {
                        ExternVal::Global(g) => imports.globals.push(g),
                        ExternVal::Table(t) => imports.tables.push(t),
                        ExternVal::Mem(m) => imports.mems.push(m),
                        ExternVal::Func(f) => imports.funcs.push(f),
                    }
                }
            }
        }

        Ok(imports)
    }
}
