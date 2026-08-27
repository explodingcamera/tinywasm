use crate::{RefType, StorageType};

/// Type of a WebAssembly value.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-valtype>
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum WasmType {
    /// A 32-bit integer.
    I32,
    /// A 64-bit integer.
    I64,
    /// A 32-bit float.
    F32,
    /// A 64-bit float.
    F64,
    /// A 128-bit vector.
    V128,
    /// A reference type.
    Ref(RefType),
}

impl From<StorageType> for WasmType {
    fn from(value: StorageType) -> Self {
        match value {
            StorageType::I8 | StorageType::I16 => Self::I32,
            StorageType::Value(value) => value,
        }
    }
}
