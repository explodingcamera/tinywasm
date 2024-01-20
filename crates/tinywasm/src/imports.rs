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
    ExternVal, ExternalKind, GlobalType, MemoryType, ModuleInstanceAddr, TableType, WasmFunction, WasmValue,
};

#[derive(Debug)]
pub(crate) enum Function {
    Host(HostFunction),
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
    Func(HostFunction),
}

/// A function
#[derive(Debug)]
pub struct ExternFunc {
    pub(crate) inner: HostFunction,
}

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

        Self::Func(HostFunction {
            func: Arc::new(inner_func),
            ty: ty.clone(),
        })
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

        Self::Func(HostFunction {
            func: Arc::new(inner_func),
            ty: ty.clone(),
        })
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

#[derive(Debug, Default)]
/// Imports for a module instance
pub struct Imports {
    values: BTreeMap<ExternName, Extern>,
    modules: BTreeMap<String, ModuleInstanceAddr>,
}

pub(crate) struct LinkedImports {
    // externs that were defined and need to be instantiated
    pub(crate) externs: BTreeMap<ExternName, Extern>,

    // externs that were linked to other modules and already exist in the store
    pub(crate) linked_externs: BTreeMap<ExternName, ExternVal>,
}

impl LinkedImports {
    pub(crate) fn get(&self, module: &str, name: &str) -> Option<&Extern> {
        self.externs.get(&ExternName {
            module: module.to_string(),
            name: name.to_string(),
        })
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

    pub(crate) fn link(self, store: &mut crate::Store, module: &crate::Module) -> Result<LinkedImports> {
        let mut links = BTreeMap::new();

        for import in module.data.imports.iter() {
            if let Some(i) = self.values.get(&ExternName {
                module: import.module.to_string(),
                name: import.name.to_string(),
            }) {
                if i.kind() != (&import.kind).into() {
                    return Err(crate::Error::InvalidImportType {
                        module: import.module.to_string(),
                        name: import.name.to_string(),
                    });
                }

                continue;
            }

            let module_addr =
                self.modules
                    .get(&import.module.to_string())
                    .ok_or_else(|| crate::Error::MissingImport {
                        module: import.module.to_string(),
                        name: import.name.to_string(),
                    })?;

            let module =
                store
                    .get_module_instance(*module_addr)
                    .ok_or_else(|| crate::Error::CouldNotResolveImport {
                        module: import.module.to_string(),
                        name: import.name.to_string(),
                    })?;

            let export =
                module
                    .exports()
                    .get_untyped(&import.name)
                    .ok_or_else(|| crate::Error::CouldNotResolveImport {
                        module: import.module.to_string(),
                        name: import.name.to_string(),
                    })?;

            // validate import
            if export.kind != (&import.kind).into() {
                return Err(crate::Error::InvalidImportType {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                });
            }

            let val = match export.kind {
                ExternalKind::Func => ExternVal::Func(export.index),
                ExternalKind::Global => ExternVal::Global(export.index),
                ExternalKind::Table => ExternVal::Table(export.index),
                ExternalKind::Memory => ExternVal::Mem(export.index),
            };

            links.insert(
                ExternName {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                },
                val,
            );
        }

        // TODO: link to other modules (currently only direct imports are supported)
        Ok(LinkedImports {
            externs: self.values,
            linked_externs: links,
        })
    }
}
