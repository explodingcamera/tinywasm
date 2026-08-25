use tinywasm_types::Shared;

/// A WebAssembly Data Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#data-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct DataInstance {
    pub(crate) data: Option<Shared<[u8]>>,
}

impl DataInstance {
    pub(crate) fn drop(&mut self) {
        self.data.take();
    }
}
