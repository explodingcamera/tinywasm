use alloc::rc::Rc;
use tinywasm_types::*;

use crate::func::HostFunction;

/// A WebAssembly Function Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#function-instances>
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct FunctionInstance {
    pub(crate) func: FunctionDef,
    pub(crate) owner: ModuleInstanceAddr, // index into store.module_instances, none for host functions
}

/// The internal representation of a function
#[derive(Clone)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum FunctionDef {
    /// A host function
    Host(Rc<HostFunction>),

    /// A pointer to a WebAssembly function
    Wasm(Rc<WasmFunction>),
}

impl FunctionDef {
    pub(crate) fn ty(&self) -> &FuncType {
        match self {
            Self::Host(f) => &f.ty,
            Self::Wasm(f) => &f.ty,
        }
    }
}

impl FunctionInstance {
    pub(crate) fn new_wasm(func: WasmFunction, owner: ModuleInstanceAddr) -> Self {
        Self { func: FunctionDef::Wasm(Rc::new(func)), owner }
    }
}
