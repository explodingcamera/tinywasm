use core::fmt::{Display, Formatter};

use crate::TinyWasmModule;
use rkyv::{
    access,
    api::high::to_bytes_in_with_alloc,
    deserialize,
    ser::{allocator::Arena, WriterExt},
    Archived,
};

const TWASM_MAGIC_PREFIX: &[u8; 4] = b"TWAS";
const TWASM_VERSION: &[u8; 2] = b"02";
#[rustfmt::skip]
const TWASM_MAGIC: [u8; 16] = [ TWASM_MAGIC_PREFIX[0], TWASM_MAGIC_PREFIX[1], TWASM_MAGIC_PREFIX[2], TWASM_MAGIC_PREFIX[3], TWASM_VERSION[0], TWASM_VERSION[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

pub use rkyv::util::AlignedVec;

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
    InvalidArchive(rkyv::rancor::Error),
}

impl Display for TwasmError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            TwasmError::InvalidMagic => write!(f, "Invalid twasm: invalid magic number"),
            TwasmError::InvalidVersion => write!(f, "Invalid twasm: invalid version"),
            TwasmError::InvalidPadding => write!(f, "Invalid twasm: invalid padding"),
            TwasmError::InvalidArchive(e) => write!(f, "Invalid twasm: {}", e),
        }
    }
}

#[cfg(feature = "std")]
extern crate std;

impl core::error::Error for TwasmError {}

impl TinyWasmModule {
    /// Creates a `TinyWasmModule` from a slice of bytes.
    pub fn from_twasm(wasm: &[u8]) -> Result<TinyWasmModule, TwasmError> {
        let len = validate_magic(wasm)?;
        let root = access::<Archived<Self>, rkyv::rancor::Error>(&wasm[len..]).map_err(TwasmError::InvalidArchive)?;
        deserialize::<TinyWasmModule, rkyv::rancor::Error>(root).map_err(TwasmError::InvalidArchive)
    }

    /// Serializes the `TinyWasmModule` into a vector of bytes.
    /// `AlignedVec` can be deferenced as a slice of bytes and
    /// implements `io::Write` when the `std` feature is enabled.
    pub fn serialize_twasm(&self) -> AlignedVec {
        let mut arena = Arena::new();
        let mut bytes = AlignedVec::new();
        <AlignedVec as WriterExt<rkyv::rancor::Error>>::pad(&mut bytes, TWASM_MAGIC.len()).unwrap();
        let mut bytes = to_bytes_in_with_alloc::<_, _, rkyv::rancor::Error>(self, bytes, arena.acquire()).unwrap();
        bytes[..TWASM_MAGIC.len()].copy_from_slice(&TWASM_MAGIC);
        bytes
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

    #[test]
    fn test_invalid_magic() {
        let wasm = TinyWasmModule::default();
        let mut twasm = wasm.serialize_twasm();
        twasm[0] = 0;
        assert!(matches!(TinyWasmModule::from_twasm(&twasm), Err(TwasmError::InvalidMagic)));
    }

    #[test]
    fn test_invalid_version() {
        let wasm = TinyWasmModule::default();
        let mut twasm = wasm.serialize_twasm();
        twasm[4] = 0;
        assert!(matches!(TinyWasmModule::from_twasm(&twasm), Err(TwasmError::InvalidVersion)));
    }
}
