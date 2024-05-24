/// A large raw wasm value, used for 128-bit values.
///
/// This is the internal representation of vector values.
///
/// See [`WasmValue`] for the public representation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct RawSimdWasmValue([u8; 16]);

impl Debug for RawSimdWasmValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "LargeRawWasmValue({})", 0)
    }
}
