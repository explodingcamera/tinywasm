use alloc::vec::Vec;

/// A WebAssembly Data Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#data-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct DataInstance {
    pub(crate) data: Option<Vec<u8>>,
}

impl DataInstance {
    pub(crate) fn new(data: Option<Vec<u8>>) -> Self {
        Self { data }
    }

    pub(crate) fn drop(&mut self) {
        self.data.is_some().then(|| self.data.take());
    }
}
