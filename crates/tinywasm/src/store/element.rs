use crate::TableElement;
use alloc::vec::Vec;

/// A WebAssembly Element Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#element-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct ElementInstance {
    pub(crate) items: Option<Vec<TableElement>>, // none is the element was dropped
}

impl ElementInstance {
    pub(crate) fn drop(&mut self) {
        self.items.take();
    }
}
