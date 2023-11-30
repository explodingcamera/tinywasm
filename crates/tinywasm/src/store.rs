use alloc::vec::Vec;
use wasmparser::FunctionBody;

use crate::{engine::Engine, module::reader::ModuleReader, Result};

/// global state that can be manipulated by WebAssembly programs
/// https://webassembly.github.io/spec/core/exec/runtime.html#store
#[derive(Debug)]
pub struct Store<'data> {
    pub data: StoreData<'data>,
    pub engine: Engine,
}

#[derive(Debug)]
pub struct StoreData<'data> {
    pub funcs: Vec<FunctionBody<'data>>,
    // pub tables: Vec<TableAddr>,
    // pub mems: Vec<MemAddr>,
    // pub globals: Vec<GlobalAddr>,
}

impl<'data> Default for StoreData<'data> {
    fn default() -> Self {
        Self {
            funcs: Vec::new(),
            // tables: Vec::new(),
            // mems: Vec::new(),
            // globals: Vec::new(),
        }
    }
}

impl<'data> Store<'data> {
    pub(crate) fn initialize(&'data mut self, reader: &'data mut ModuleReader) -> Result<()> {
        let code = reader.code_section.take().ok_or_else(|| {
            crate::Error::Other("Module must have a code section to initialize the store".into())
        })?;

        self.data.funcs = code.functions;
        Ok(())
    }
}

impl Default for Store<'_> {
    fn default() -> Self {
        Self {
            data: StoreData::default(),
            engine: Engine::default(),
        }
    }
}
