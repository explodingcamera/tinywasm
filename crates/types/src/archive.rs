use core::fmt::{Display, Formatter};

use alloc::vec::Vec;

use crate::Module;

#[rustfmt::skip]
const TWASM_MAGIC: [u8; 16] = [ TWASM_MAGIC_PREFIX[0], TWASM_MAGIC_PREFIX[1], TWASM_MAGIC_PREFIX[2], TWASM_MAGIC_PREFIX[3], TWASM_VERSION[0], TWASM_VERSION[1], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const TWASM_MAGIC_PREFIX: &[u8; 4] = b"TWAS";
const TWASM_VERSION: &[u8; 2] = b"06";

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
    use crate::Operand128Idx;
    use crate::{
        AbstractHeapType, ConstInstruction, Global, GlobalType, Instruction, ModuleFuncIdx, ModuleInner, Operand128,
        RefType, Shared, WasmFunction, WasmType,
    };
    use alloc::boxed::Box;

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
        let constant = Operand128Idx::new(0);
        function.data.operands128 = Box::new([Operand128::<[u8; 16]>::new(bytes).cast()]);
        function.instructions = Box::new([Instruction::Const128(constant), Instruction::I8x16Shuffle(constant)]);
        let module = Module::from(ModuleInner { funcs: Box::new([Shared::new(function)]), ..ModuleInner::default() });

        let archive = module.serialize_twasm().expect("serialize archive");
        assert_eq!(&archive[..6], b"TWAS06");
        let decoded = Module::try_from_twasm(&archive).expect("deserialize archive");
        let function = &decoded.funcs[0];

        for instruction in function.instructions.iter() {
            let index = match instruction {
                Instruction::Const128(index) | Instruction::I8x16Shuffle(index) => *index,
                _ => panic!("unexpected instruction"),
            };
            assert_eq!(index.resolve(&function.data).value(), bytes);
        }
    }

    #[test]
    fn const_reference_expressions_round_trip_archive() {
        let null_type = RefType::new_abstract(true, AbstractHeapType::Extern);
        let module = Module::from(ModuleInner {
            globals: Box::new([
                Global {
                    ty: GlobalType::new(WasmType::Ref(null_type), false),
                    init: Box::new([ConstInstruction::RefNull(null_type)]),
                },
                Global {
                    ty: GlobalType::new(WasmType::Ref(RefType::FUNCREF), false),
                    init: Box::new([ConstInstruction::RefFunc(ModuleFuncIdx::new(7))]),
                },
                Global {
                    ty: GlobalType::new(WasmType::Ref(RefType::new_abstract(false, AbstractHeapType::I31)), false),
                    init: Box::new([ConstInstruction::I32Const(42), ConstInstruction::RefI31]),
                },
            ]),
            ..ModuleInner::default()
        });

        let archive = module.serialize_twasm().expect("serialize archive");
        let decoded = Module::try_from_twasm(&archive).expect("deserialize archive");

        assert!(matches!(decoded.globals[0].init.as_ref(), [ConstInstruction::RefNull(ty)] if *ty == null_type));
        assert!(matches!(
            decoded.globals[1].init.as_ref(),
            [ConstInstruction::RefFunc(index)] if index.index() == 7
        ));
        assert!(matches!(decoded.globals[2].init.as_ref(), [ConstInstruction::I32Const(42), ConstInstruction::RefI31]));
    }
}
