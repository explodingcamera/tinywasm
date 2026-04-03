use crate::interpreter::TinyWasmValue;
use core::cell::Cell;
use tinywasm_types::*;

/// A WebAssembly Global Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#global-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct GlobalInstance {
    pub(crate) value: Cell<TinyWasmValue>,
    pub(crate) ty: GlobalType,
}

impl GlobalInstance {
    pub(crate) fn new(ty: GlobalType, value: TinyWasmValue) -> Self {
        Self { ty, value: value.into() }
    }
}
