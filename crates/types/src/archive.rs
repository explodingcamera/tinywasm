use core::fmt::{Display, Formatter};

use alloc::vec::Vec;

use crate::Module;

#[rustfmt::skip]
const TWASM_MAGIC: [u8; 16] = [ TWASM_MAGIC_PREFIX[0], TWASM_MAGIC_PREFIX[1], TWASM_MAGIC_PREFIX[2], TWASM_MAGIC_PREFIX[3], TWASM_VERSION[0], TWASM_VERSION[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const TWASM_MAGIC_PREFIX: &[u8; 4] = b"TWAS";
const TWASM_VERSION: &[u8; 2] = b"03";

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
    InvalidArchive(postcard::Error),
}

impl Display for TwasmError {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "Invalid twasm: invalid magic number"),
            Self::InvalidVersion => write!(f, "Invalid twasm: invalid version"),
            Self::InvalidPadding => write!(f, "Invalid twasm: invalid padding"),
            Self::InvalidArchive(e) => write!(f, "Invalid twasm: {e}"),
        }
    }
}

#[cfg(feature = "std")]
extern crate std;

impl core::error::Error for TwasmError {}

impl Module {
    /// Creates a [`Module`] from a slice of bytes.
    pub fn try_from_twasm(wasm: &[u8]) -> Result<Self, TwasmError> {
        let len = validate_magic(wasm)?;
        postcard::from_bytes(&wasm[len..]).map_err(TwasmError::InvalidArchive)
    }

    /// Serializes the [`Module`] into a vector of bytes.
    pub fn serialize_twasm(&self) -> Result<Vec<u8>, TwasmError> {
        let buf = Vec::from(TWASM_MAGIC);
        postcard::to_extend(self, buf).map_err(TwasmError::InvalidArchive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_magic() {
        let wasm = Module::default();
        let mut twasm = wasm.serialize_twasm().expect("should serialize");
        twasm[0] = 0;
        assert!(matches!(Module::try_from_twasm(&twasm), Err(TwasmError::InvalidMagic)));
    }

    #[test]
    fn test_invalid_version() {
        let wasm = Module::default();
        let mut twasm = wasm.serialize_twasm().expect("should serialize");
        twasm[4] = 0;
        assert!(matches!(Module::try_from_twasm(&twasm), Err(TwasmError::InvalidVersion)));
    }
}
