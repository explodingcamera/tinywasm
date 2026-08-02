use alloc::{rc::Rc, sync::Arc};
use tinywasm_types::*;

use crate::func::HostFunction;

/// A WebAssembly Function Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct FunctionInstance {
    pub(crate) type_addr: TypeAddr,
    pub(crate) kind: FunctionKind,
}

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum FunctionKind {
    /// A host function
    Host(Rc<HostFunction>),

    /// A pointer to a WebAssembly function
    Wasm(WasmFunctionInstance),
}

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct WasmFunctionInstance {
    pub(crate) func: Arc<WasmFunction>,
    pub(crate) owner: ModuleInstanceId,
}
