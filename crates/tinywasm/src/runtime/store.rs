use alloc::vec::Vec;

use super::FuncInst;

/// global state that can be manipulated by WebAssembly programs
/// https://webassembly.github.io/spec/core/exec/runtime.html#store
pub struct Store {
    pub funcs: Vec<FuncInst>,
    // tables: Vec<TableType>,
    // mems: Vec<MemoryType>,
    // globals: Vec<GlobalType>,
    // elems: Vec<Element>,
    // data: Vec<Data>,
}
