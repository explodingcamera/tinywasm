use crate::Function;
use alloc::rc::Rc;
use tinywasm_types::*;

#[derive(Debug, Clone)]
/// A WebAssembly Function Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
pub(crate) struct FunctionInstance {
    pub(crate) func: Function,
    pub(crate) owner: ModuleInstanceAddr, // index into store.module_instances, none for host functions
}

impl FunctionInstance {
    pub(crate) fn new_wasm(func: WasmFunction, owner: ModuleInstanceAddr) -> Self {
        Self { func: Function::Wasm(Rc::new(func)), owner }
    }
}
