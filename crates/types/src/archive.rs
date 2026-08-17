use core::fmt::{Display, Formatter};

use alloc::vec::Vec;

use crate::Module;

#[rustfmt::skip]
const TWASM_MAGIC: [u8; 16] = [ TWASM_MAGIC_PREFIX[0], TWASM_MAGIC_PREFIX[1], TWASM_MAGIC_PREFIX[2], TWASM_MAGIC_PREFIX[3], TWASM_VERSION[0], TWASM_VERSION[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const TWASM_MAGIC_PREFIX: &[u8; 4] = b"TWAS";
const TWASM_VERSION: &[u8; 2] = b"05";

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

#[derive(Debug, PartialEq, Eq)]
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

impl core::error::Error for TwasmError {}

impl Module {
    /// Creates a [`Module`] from internal `twasm` archive bytes.
    ///
    /// Archives are version-specific and are not validated as untrusted input.
    /// Only load archives from a trusted source.
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
    use crate::{Instruction, ModuleInner, V128Operand, WasmFunction};
    use crate::{OperandIdx, OperandType};
    use alloc::{boxed::Box, sync::Arc, vec};

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

    #[test]
    fn v128_operands_round_trip_archive() {
        let bytes = [0x00, 0x01, 0x02, 0x03, 0x7f, 0x80, 0xfe, 0xff, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x90];
        let mut function = WasmFunction::default();
        let constant = OperandIdx::new(0);
        function.data.operands128 = vec![V128Operand { value: bytes }.encode()].into_boxed_slice();
        function.instructions = vec![Instruction::Const128(constant), Instruction::I8x16Shuffle(constant)].into();
        let module = Module::from(ModuleInner { funcs: Box::new([Arc::new(function)]), ..ModuleInner::default() });

        let archive = module.serialize_twasm().expect("serialize archive");
        assert_eq!(&archive[..6], b"TWAS05");
        let decoded = Module::try_from_twasm(&archive).expect("deserialize archive");
        let function = &decoded.funcs[0];

        for instruction in function.instructions.iter() {
            let index = match instruction {
                Instruction::Const128(index) | Instruction::I8x16Shuffle(index) => *index,
                _ => panic!("unexpected instruction"),
            };
            assert_eq!(index.get(&function.data).value, bytes);
        }
    }
}
