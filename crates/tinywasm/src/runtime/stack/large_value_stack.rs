use alloc::vec::Vec;

use crate::runtime::LargeRawWasmValue;

#[derive(Debug)]
pub(crate) struct LargeValueStack(Vec<LargeRawWasmValue>);

impl Default for LargeValueStack {
    fn default() -> Self {
        Self(Vec::new())
    }
}
