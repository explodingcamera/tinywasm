use crate::Result;
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
};
use tinywasm_types::{Global, GlobalType, WasmValue};

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
}

impl Imports {
    /// Create a new empty import set
    pub fn new() -> Self {
        Imports {
            values: BTreeMap::new(),
        }
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

    pub(crate) fn get(&self, module: &str, name: &str) -> Option<&Extern> {
        self.values.get(&ExternName {
            module: module.to_string(),
            name: name.to_string(),
        })
    }
}
