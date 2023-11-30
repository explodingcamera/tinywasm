use alloc::vec::Vec;
use wasmparser::FunctionBody;

use crate::{module::reader::ModuleReader, Result};

/// global state that can be manipulated by WebAssembly programs
/// https://webassembly.github.io/spec/core/exec/runtime.html#store
#[derive(Debug, Default)]
pub struct Store<'data> {
    pub(crate) data: StoreData<'data>,
}

#[derive(Debug, Default)]
pub struct StoreData<'data> {
    pub funcs: Vec<FunctionBody<'data>>,
    // pub tables: Vec<TableAddr>,
    // pub mems: Vec<MemAddr>,
    // pub globals: Vec<GlobalAddr>,
    // pub elems: Vec<ElmAddr>,
    // pub datas: Vec<DataAddr>,
}

impl<'data> Store<'data> {
    /// Initialize the store with global state from the given module
    pub(crate) fn initialize(&'data mut self, reader: &ModuleReader<'data>) -> Result<()> {
        let code = reader.code_section.clone().ok_or_else(|| {
            crate::Error::Other("Module must have a code section to initialize the store".into())
        })?;

        self.data.funcs = code.functions;
        Ok(())
    }
}
