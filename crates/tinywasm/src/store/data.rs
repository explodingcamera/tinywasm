use alloc::vec::Vec;
use tinywasm_types::*;

/// A WebAssembly Data Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#data-instances>
#[derive(Debug)]
pub(crate) struct DataInstance {
    pub(crate) data: Option<Vec<u8>>,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl DataInstance {
    pub(crate) fn new(data: Option<Vec<u8>>, owner: ModuleInstanceAddr) -> Self {
        Self { data, _owner: owner }
    }

    pub(crate) fn drop(&mut self) {
        self.data.is_some().then(|| self.data.take());
    }
}
