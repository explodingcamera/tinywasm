use tinywasm_types::*;

use crate::func::HostFunction;

/// A WebAssembly Function Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct FunctionInstance {
    pub(crate) type_addr: TypeAddr,
    pub(crate) inner: FunctionInstanceInner,
}

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum FunctionInstanceInner {
    /// A host function
    Host(HostFunction),

    /// A pointer to a WebAssembly function
    Wasm(WasmFunctionInstance),
}

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct WasmFunctionInstance {
    pub(crate) func: Shared<WasmFunction>,
    pub(crate) owner: ModuleInstanceId,
}
