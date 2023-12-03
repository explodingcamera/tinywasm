use alloc::{boxed::Box, format};
use tinywasm_types::Function;

use crate::{runtime::Runtime, Error, Result};

/// global state that can be manipulated by WebAssembly programs
/// https://webassembly.github.io/spec/core/exec/runtime.html#store
#[derive(Debug, Default)]
pub struct Store {
    pub(crate) data: StoreData,
    pub(crate) runtime: Runtime,
}

// #[derive(Debug)]
// pub struct FunctionInstance {
//     pub(crate) func: Function,
//     pub(crate) module: usize,
// }

#[derive(Debug, Default)]
pub struct StoreData {
    pub funcs: Box<[Function]>,
    // pub tables: Vec<TableAddr>,
    // pub mems: Vec<MemAddr>,
    // pub globals: Vec<GlobalAddr>,
    // pub elems: Vec<ElmAddr>,
    // pub datas: Vec<DataAddr>,
}

impl Store {
    /// Initialize the store with global state from the given module
    pub(crate) fn initialize(&mut self, data: StoreData) -> Result<()> {
        self.data = data;
        Ok(())
    }

    pub(crate) fn get_func(&self, index: usize) -> Result<&Function> {
        self.data
            .funcs
            .get(index)
            .ok_or_else(|| Error::Other(format!("function {} not found", index)))
    }
}
