use crate::Result;
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};
use tinywasm_types::{Global, GlobalType, ModuleInstanceAddr, WasmValue};

#[derive(Debug)]
#[non_exhaustive]
/// An external value
pub enum Extern {
    /// A global value
    Global(Global),
    // Func(HostFunc),
    // Table(Table),
}

impl Extern {
    /// Create a new global import
    pub fn global(val: WasmValue, mutable: bool) -> Self {
        Self::Global(Global {
            ty: GlobalType {
                ty: val.val_type(),
                mutable,
            },
            init: val.const_instr(),
        })
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
    pub(crate) values: BTreeMap<ExternName, Extern>,
}

impl LinkedImports {
    pub(crate) fn get(&self, module: &str, name: &str) -> Option<&Extern> {
        self.values.get(&ExternName {
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
    /// This will automatically link all imported values
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
        let values = self.values;
        Ok(LinkedImports { values })
    }
}
