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
    ExternVal, ExternalKind, FuncAddr, GlobalAddr, GlobalType, Import, MemAddr, MemoryType, ModuleInstanceAddr,
    TableAddr, TableType, WasmFunction, WasmValue,
};

/// The internal representation of a function
#[derive(Debug)]
pub enum Function {
    /// A host function
    Host(HostFunction),

    /// A function defined in WebAssembly
    Wasm(WasmFunction),
}

/// A host function
pub struct HostFunction {
    pub(crate) ty: tinywasm_types::FuncType,
    pub(crate) func: Arc<dyn Fn(&mut crate::Store, &[WasmValue]) -> Result<Vec<WasmValue>> + 'static + Send + Sync>,
}

impl Debug for HostFunction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HostFunction")
            .field("ty", &self.ty)
            .field("func", &"...")
            .finish()
    }
}

#[derive(Debug)]
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
#[derive(Debug)]
pub struct ExternFunc(pub(crate) HostFunction);

/// A global value
#[derive(Debug)]
pub struct ExternGlobal {
    pub(crate) ty: GlobalType,
    pub(crate) val: WasmValue,
}

/// A table
#[derive(Debug)]
pub struct ExternTable {
    pub(crate) ty: TableType,
    pub(crate) val: WasmValue,
}

/// A memory
#[derive(Debug)]
pub struct ExternMemory {
    pub(crate) ty: MemoryType,
}

impl Extern {
    /// Create a new global import
    pub fn global(val: WasmValue, mutable: bool) -> Self {
        Self::Global(ExternGlobal {
            ty: GlobalType {
                ty: val.val_type(),
                mutable,
            },
            val,
        })
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

        Self::Func(Function::Host(HostFunction {
            func: Arc::new(inner_func),
            ty: ty.clone(),
        }))
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

        let ty = tinywasm_types::FuncType {
            params: P::val_types(),
            results: R::val_types(),
        };

        Self::Func(Function::Host(HostFunction {
            func: Arc::new(inner_func),
            ty,
        }))
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
        Self {
            module: import.module.to_string(),
            name: import.name.to_string(),
        }
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

pub(crate) enum ResolvedImport {
    Extern(Extern),
    Store(ExternVal),
}

impl ResolvedImport {
    fn initialize(self, store: &mut crate::Store, idx: ModuleInstanceAddr) -> Result<ExternVal> {
        match self {
            Self::Extern(extern_) => match extern_ {
                Extern::Global(global) => {
                    let addr = store.add_global(global.ty, global.val.into(), idx)?;
                    Ok(ExternVal::Global(addr))
                }
                Extern::Table(table) => {
                    // todo: do something with the initial value
                    let addr = store.add_table(table.ty, idx)?;
                    Ok(ExternVal::Table(addr))
                }
                Extern::Memory(memory) => {
                    let addr = store.add_mem(memory.ty, idx)?;
                    Ok(ExternVal::Mem(addr))
                }
                Extern::Func(func) => {
                    let addr = store.add_func(func, idx)?;
                    Ok(ExternVal::Func(addr))
                }
            },
            Self::Store(extern_val) => Ok(extern_val.clone()),
        }
    }
}

pub(crate) struct ResolvedImports {
    pub(crate) globals: Vec<ResolvedExtern<GlobalAddr, ExternGlobal>>,
    pub(crate) tables: Vec<ResolvedExtern<TableAddr, ExternTable>>,
    pub(crate) mems: Vec<ResolvedExtern<MemAddr, ExternMemory>>,
    pub(crate) funcs: Vec<ResolvedExtern<FuncAddr, ExternFunc>>,
}

impl ResolvedImports {
    pub(crate) fn new() -> Self {
        Self {
            globals: Vec::new(),
            tables: Vec::new(),
            mems: Vec::new(),
            funcs: Vec::new(),
        }
    }

    pub(crate) fn globals(&self) -> &[ResolvedExtern<GlobalAddr, ExternGlobal>] {
        &self.globals
    }

    pub(crate) fn tables(&self) -> &[ResolvedExtern<TableAddr, ExternTable>] {
        &self.tables
    }

    pub(crate) fn mems(&self) -> &[ResolvedExtern<MemAddr, ExternMemory>] {
        &self.mems
    }

    pub(crate) fn funcs(&self) -> &[ResolvedExtern<FuncAddr, ExternFunc>] {
        &self.funcs
    }
}

impl Imports {
    /// Create a new empty import set
    pub fn new() -> Self {
        Imports {
            values: BTreeMap::new(),
            modules: BTreeMap::new(),
        }
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
        self.values.insert(
            ExternName {
                module: module.to_string(),
                name: name.to_string(),
            },
            value,
        );
        Ok(self)
    }

    pub(crate) fn take(&mut self, store: &mut crate::Store, import: &Import) -> Option<ResolvedImport> {
        // TODO: compare types

        let name = ExternName::from(import);
        if let Some(v) = self.values.remove(&name) {
            return Some(ResolvedImport::Extern(v));
        }

        return None;

        // TODO: allow linking to other modules
        // if let Some(module_addr) = self.modules.get(&name.module) {
        //     let Some(module) = store.get_module_instance(*module_addr) else {
        //         return None;
        //     };

        //     let export = module.exports().get_untyped(&name.name)?;
        // };

        // then check if the import is defined
    }

    pub(crate) fn link(mut self, store: &mut crate::Store, module: &crate::Module) -> Result<ResolvedImports> {
        let mut imports = ResolvedImports::new();

        for import in module.data.imports.iter() {
            let Some(val) = self.take(store, import) else {
                return Err(crate::Error::MissingImport {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                });
            };

            // validate import
            // if export.kind != (&import.kind).into() {
            //     return Err(crate::Error::InvalidImportType {
            //         module: import.module.to_string(),
            //         name: import.name.to_string(),
            //     });
            // }

            // let val = match export.kind {
            //     ExternalKind::Func => ExternVal::Func(export.index),
            //     ExternalKind::Global => ExternVal::Global(export.index),
            //     ExternalKind::Table => ExternVal::Table(export.index),
            //     ExternalKind::Memory => ExternVal::Mem(export.index),
            // };

            // imports.0.insert(
            //     ExternName {
            //         module: import.module.to_string(),
            //         name: import.name.to_string(),
            //     },
            //     ResolvedImport::Store(val),
            // );
        }

        Ok(imports)
    }
}
