use core::{fmt::Debug, simd::Simd};

/// A large raw wasm value, used for 128-bit values.
///
/// This is the internal representation of vector values.
///
/// See [`WasmValue`] for the public representation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct RawSimdWasmValue(Simd<u8, 16>); // wasm has up to 16 8 bit lanes

impl Debug for RawSimdWasmValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "LargeRawWasmValue({})", 0)
    }
}

impl From<u128> for RawSimdWasmValue {
    fn from(value: u128) -> Self {
        Self(value.to_le_bytes().into())
    }
}

impl From<RawSimdWasmValue> for u128 {
    fn from(value: RawSimdWasmValue) -> Self {
        u128::from_le_bytes(value.0.into())
    }
}
