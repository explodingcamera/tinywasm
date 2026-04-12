use alloc::rc::Rc;
use tinywasm_types::*;

use crate::func::HostFunction;

/// A WebAssembly Function Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum FunctionInstance {
    /// A host function
    Host(Rc<HostFunction>),

    /// A pointer to a WebAssembly function
    Wasm(WasmFunctionInstance),
}

impl FunctionInstance {
    #[inline]
    pub(crate) fn ty(&self) -> &FuncType {
        match self {
            Self::Host(f) => &f.ty,
            Self::Wasm(f) => f.ty(),
        }
    }
}

impl FunctionInstance {
    pub(crate) fn new_wasm(func: WasmFunction, owner: ModuleInstanceAddr) -> Self {
        Self::Wasm(WasmFunctionInstance { func: Rc::new(func), owner })
    }
}

#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct WasmFunctionInstance {
    pub(crate) func: Rc<WasmFunction>,
    pub(crate) owner: ModuleInstanceAddr,
}

impl WasmFunctionInstance {
    #[inline]
    pub(crate) fn ty(&self) -> &FuncType {
        &self.func.ty
    }
}
