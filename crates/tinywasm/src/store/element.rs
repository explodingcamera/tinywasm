use crate::interpreter::ValueRef;
use alloc::vec::Vec;
use tinywasm_types::RefType;

/// A WebAssembly Element Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#element-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct ElementInstance {
    pub(crate) items: Option<Vec<ValueRef>>, // none is the element was dropped
    pub(crate) ty: RefType,
}

impl ElementInstance {
    pub(crate) fn drop(&mut self) {
        self.items.take();
    }
}
