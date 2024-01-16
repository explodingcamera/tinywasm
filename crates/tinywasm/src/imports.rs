use crate::Result;
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};
use tinywasm_types::{ExternVal, ExternalKind, GlobalType, MemoryType, ModuleInstanceAddr, TableType, WasmValue};

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
    Func,
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

    pub(crate) fn kind(&self) -> ExternalKind {
        match self {
            Self::Global(_) => ExternalKind::Global,
            Self::Table(_) => ExternalKind::Table,
            Self::Memory(_) => ExternalKind::Memory,
            Self::Func => ExternalKind::Func,
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

            let export = module.exports().get_untyped(&import.name.to_string()).ok_or_else(|| {
                crate::Error::CouldNotResolveImport {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                }
            })?;

            // validate import
            if export.kind != (&import.kind).into() {
                return Err(crate::Error::InvalidImportType {
                    module: import.module.to_string(),
                    name: import.name.to_string(),
                });
            }

            let val = match export.kind {
                ExternalKind::Func => ExternVal::Func(export.index.clone()),
                ExternalKind::Global => ExternVal::Global(export.index.clone()),
                ExternalKind::Table => ExternVal::Table(export.index.clone()),
                ExternalKind::Memory => ExternVal::Mem(export.index.clone()),
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
