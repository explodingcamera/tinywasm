use crate::interpreter::TinyWasmValue;
use core::cell::Cell;
use tinywasm_types::*;

/// A WebAssembly Global Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#global-instances>
#[derive(Debug)]
pub(crate) struct GlobalInstance {
    pub(crate) value: Cell<TinyWasmValue>,
    pub(crate) ty: GlobalType,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl GlobalInstance {
    pub(crate) fn new(ty: GlobalType, value: TinyWasmValue, owner: ModuleInstanceAddr) -> Self {
        Self { ty, value: value.into(), _owner: owner }
    }
}
