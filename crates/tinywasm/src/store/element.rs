use crate::{Trap, interpreter::ValueRef};
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

    pub(crate) fn items_range(&self, offset: usize, len: usize) -> Result<&[ValueRef], Trap> {
        let items = self.items.as_deref().unwrap_or(&[]);
        let end = offset.checked_add(len).filter(|end| *end <= items.len()).ok_or_else(|| {
            core::hint::cold_path();
            Trap::TableOutOfBounds { offset, len, max: items.len() }
        })?;
        Ok(&items[offset..end])
    }
}
