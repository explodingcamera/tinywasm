use core::fmt::{Display, Formatter};

use crate::TinyWasmModule;
use rkyv::{
    check_archived_root,
    ser::{serializers::AllocSerializer, Serializer},
    Deserialize,
};

const TWASM_MAGIC_PREFIX: &[u8; 4] = b"TWAS";
const TWASM_VERSION: &[u8; 2] = b"01";
#[rustfmt::skip]
const TWASM_MAGIC: [u8; 16] = [ TWASM_MAGIC_PREFIX[0], TWASM_MAGIC_PREFIX[1], TWASM_MAGIC_PREFIX[2], TWASM_MAGIC_PREFIX[3], TWASM_VERSION[0], TWASM_VERSION[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

pub use rkyv::AlignedVec;

fn validate_magic(wasm: &[u8]) -> Result<usize, TwasmError> {
    if wasm.len() < TWASM_MAGIC.len() || &wasm[..TWASM_MAGIC_PREFIX.len()] != TWASM_MAGIC_PREFIX {
        return Err(TwasmError::InvalidMagic);
    }
    if &wasm[TWASM_MAGIC_PREFIX.len()..TWASM_MAGIC_PREFIX.len() + TWASM_VERSION.len()] != TWASM_VERSION {
        return Err(TwasmError::InvalidVersion);
    }
    if wasm[TWASM_MAGIC_PREFIX.len() + TWASM_VERSION.len()..TWASM_MAGIC.len()] != [0; 10] {
        return Err(TwasmError::InvalidPadding);
    }

    Ok(TWASM_MAGIC.len())
}

#[derive(Debug)]
pub enum TwasmError {
    InvalidMagic,
    InvalidVersion,
    InvalidPadding,
    InvalidArchive,
}

impl Display for TwasmError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            TwasmError::InvalidMagic => write!(f, "Invalid twasm: invalid magic number"),
            TwasmError::InvalidVersion => write!(f, "Invalid twasm: invalid version"),
            TwasmError::InvalidPadding => write!(f, "Invalid twasm: invalid padding"),
            TwasmError::InvalidArchive => write!(f, "Invalid twasm: invalid archive"),
        }
    }
}

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
impl std::error::Error for TwasmError {}

impl TinyWasmModule {
    /// Creates a TinyWasmModule from a slice of bytes.
    pub fn from_twasm(wasm: &[u8]) -> Result<TinyWasmModule, TwasmError> {
        let len = validate_magic(wasm)?;
        let root = check_archived_root::<Self>(&wasm[len..]).map_err(|_e| TwasmError::InvalidArchive)?;
        Ok(root.deserialize(&mut rkyv::Infallible).unwrap())
    }

    /// Serializes the TinyWasmModule into a vector of bytes.
    /// AlignedVec can be deferenced as a slice of bytes and
    /// implements io::Write when the `std` feature is enabled.
    pub fn serialize_twasm(&self) -> rkyv::AlignedVec {
        let mut serializer = AllocSerializer::<0>::default();
        serializer.pad(TWASM_MAGIC.len()).unwrap();
        serializer.serialize_value(self).unwrap();
        let mut out = serializer.into_serializer().into_inner();
        out[..TWASM_MAGIC.len()].copy_from_slice(&TWASM_MAGIC);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize() {
        let wasm = TinyWasmModule::default();
        let twasm = wasm.serialize_twasm();
        let wasm2 = TinyWasmModule::from_twasm(&twasm).unwrap();
        assert_eq!(wasm, wasm2);
    }
}
