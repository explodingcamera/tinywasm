use crate::TableElement;
use alloc::vec::Vec;
use tinywasm_types::*;

/// A WebAssembly Element Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#element-instances>
#[derive(Debug)]
pub(crate) struct ElementInstance {
    pub(crate) kind: ElementKind,
    pub(crate) items: Option<Vec<TableElement>>, // none is the element was dropped
    _owner: ModuleInstanceAddr,                  // index into store.module_instances
}

impl ElementInstance {
    pub(crate) fn new(kind: ElementKind, owner: ModuleInstanceAddr, items: Option<Vec<TableElement>>) -> Self {
        Self { kind, _owner: owner, items }
    }
}
