use super::{FuncAddr, GlobalAddr, LabelAddr, LocalAddr, TableAddr, TypeAddr, ValType};
use crate::{ConstIdx, DataAddr, ElemAddr, MemAddr};

/// Represents a memory immediate in a WebAssembly memory instruction.
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "archive", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]

pub struct MemoryArg([u8; 12]);

impl MemoryArg {
    pub fn new(offset: u64, mem_addr: MemAddr) -> Self {
        let mut bytes = [0; 12];
        bytes[0..8].copy_from_slice(&offset.to_le_bytes());
        bytes[8..12].copy_from_slice(&mem_addr.to_le_bytes());
        Self(bytes)
    }

    pub fn offset(&self) -> u64 {
        u64::from_le_bytes(self.0[0..8].try_into().expect("invalid offset"))
    }

    pub fn mem_addr(&self) -> MemAddr {
        MemAddr::from_le_bytes(self.0[8..12].try_into().expect("invalid mem_addr"))
    }
}

type BrTableDefault = u32;
type BrTableLen = u32;
type EndOffset = u32;
type ElseOffset = u32;

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "archive", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
pub enum ConstInstruction {
    I32Const(i32),
    I64Const(i64),
    F32Const(f32),
    F64Const(f64),
    GlobalGet(GlobalAddr),
    RefNull(ValType),
    RefFunc(FuncAddr),
}

/// A WebAssembly Instruction
///
/// These are our own internal bytecode instructions so they may not match the spec exactly.
/// Wasm Bytecode can map to multiple of these instructions.
///
/// # Differences to the spec
/// * `br_table` stores the jump labels in the following `br_label` instructions to keep this enum small.
/// * Lables/Blocks: we store the label end offset in the instruction itself and use `EndBlockFrame` to mark the end of a block.
///   This makes it easier to implement the label stack iteratively.
///
/// See <https://webassembly.github.io/spec/core/binary/instructions.html>
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "archive", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
// should be kept as small as possible (16 bytes max)
#[rustfmt::skip]
pub enum Instruction {
    LocalCopy32(LocalAddr, LocalAddr), LocalCopy64(LocalAddr, LocalAddr), LocalCopy128(LocalAddr, LocalAddr), LocalCopyRef(LocalAddr, LocalAddr),
    // LocalsStore32(LocalAddr, LocalAddr, u32, MemAddr), LocalsStore64(LocalAddr, LocalAddr, u32, MemAddr), LocalsStore128(LocalAddr, LocalAddr, u32, MemAddr), LocalsStoreRef(LocalAddr, LocalAddr, u32, MemAddr),

    // > Control Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#control-instructions>
    Unreachable,
    Nop,

    Block(EndOffset), 
    BlockWithType(ValType, EndOffset),
    BlockWithFuncType(TypeAddr, EndOffset),
 
    Loop(EndOffset),
    LoopWithType(ValType, EndOffset),
    LoopWithFuncType(TypeAddr, EndOffset),

    If(ElseOffset, EndOffset),
    IfWithType(ValType, ElseOffset, EndOffset),
    IfWithFuncType(TypeAddr, ElseOffset, EndOffset),

    Else(EndOffset),
    EndBlockFrame,
    Br(LabelAddr),
    BrIf(LabelAddr),
    BrTable(BrTableDefault, BrTableLen), // has to be followed by multiple BrLabel instructions
    BrLabel(LabelAddr),
    Return,
    Call(FuncAddr),
    CallIndirect(TypeAddr, TableAddr),
    // ReturnCall(FuncAddr),
    // ReturnCallIndirect(TypeAddr, TableAddr),
 
    // > Parametric Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#parametric-instructions>
    Drop32, Select32,
    Drop64, Select64,
    Drop128, Select128,
    DropRef, SelectRef,

    // > Variable Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#variable-instructions>
    GlobalGet(GlobalAddr),
    LocalGet32(LocalAddr), LocalSet32(LocalAddr), LocalTee32(LocalAddr), GlobalSet32(GlobalAddr),
    LocalGet64(LocalAddr), LocalSet64(LocalAddr), LocalTee64(LocalAddr), GlobalSet64(GlobalAddr),
    LocalGet128(LocalAddr), LocalSet128(LocalAddr), LocalTee128(LocalAddr), GlobalSet128(GlobalAddr),
    LocalGetRef(LocalAddr), LocalSetRef(LocalAddr), LocalTeeRef(LocalAddr), GlobalSetRef(GlobalAddr),

    // > Memory Instructions
    I32Load(MemoryArg),
    I64Load(MemoryArg),
    F32Load(MemoryArg),
    F64Load(MemoryArg),
    I32Load8S(MemoryArg),
    I32Load8U(MemoryArg),
    I32Load16S(MemoryArg),
    I32Load16U(MemoryArg),
    I64Load8S(MemoryArg),
    I64Load8U(MemoryArg),
    I64Load16S(MemoryArg),
    I64Load16U(MemoryArg),
    I64Load32S(MemoryArg),
    I64Load32U(MemoryArg),
    I32Store(MemoryArg),
    I64Store(MemoryArg),
    F32Store(MemoryArg),
    F64Store(MemoryArg),
    I32Store8(MemoryArg),
    I32Store16(MemoryArg),
    I64Store8(MemoryArg),
    I64Store16(MemoryArg),
    I64Store32(MemoryArg),
    MemorySize(MemAddr),
    MemoryGrow(MemAddr),

    // > Constants
    I32Const(i32),
    I64Const(i64),
    F32Const(f32),
    F64Const(f64),

    // > Reference Types
    RefNull(ValType),
    RefFunc(FuncAddr),
    RefIsNull,
 
    // > Numeric Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#numeric-instructions>
    I32Eqz, I32Eq, I32Ne, I32LtS, I32LtU, I32GtS, I32GtU, I32LeS, I32LeU, I32GeS, I32GeU,
    I64Eqz, I64Eq, I64Ne, I64LtS, I64LtU, I64GtS, I64GtU, I64LeS, I64LeU, I64GeS, I64GeU,
    // Comparisons
    F32Eq, F32Ne, F32Lt, F32Gt, F32Le, F32Ge,
    F64Eq, F64Ne, F64Lt, F64Gt, F64Le, F64Ge,
    I32Clz, I32Ctz, I32Popcnt, I32Add, I32Sub, I32Mul, I32DivS, I32DivU, I32RemS, I32RemU,
    I64Clz, I64Ctz, I64Popcnt, I64Add, I64Sub, I64Mul, I64DivS, I64DivU, I64RemS, I64RemU,
    // Bitwise
    I32And, I32Or, I32Xor, I32Shl, I32ShrS, I32ShrU, I32Rotl, I32Rotr,
    I64And, I64Or, I64Xor, I64Shl, I64ShrS, I64ShrU, I64Rotl, I64Rotr,
    // Floating Point
    F32Abs, F32Neg, F32Ceil, F32Floor, F32Trunc, F32Nearest, F32Sqrt, F32Add, F32Sub, F32Mul, F32Div, F32Min, F32Max, F32Copysign,
    F64Abs, F64Neg, F64Ceil, F64Floor, F64Trunc, F64Nearest, F64Sqrt, F64Add, F64Sub, F64Mul, F64Div, F64Min, F64Max, F64Copysign,
    I32WrapI64, I32TruncF32S, I32TruncF32U, I32TruncF64S, I32TruncF64U, I32Extend8S, I32Extend16S,
    I64Extend8S, I64Extend16S, I64Extend32S, I64ExtendI32S, I64ExtendI32U, I64TruncF32S, I64TruncF32U, I64TruncF64S, I64TruncF64U,
    F32ConvertI32S, F32ConvertI32U, F32ConvertI64S, F32ConvertI64U, F32DemoteF64,
    F64ConvertI32S, F64ConvertI32U, F64ConvertI64S, F64ConvertI64U, F64PromoteF32,
    // Reinterpretations (noops at runtime)
    I32ReinterpretF32, I64ReinterpretF64, F32ReinterpretI32, F64ReinterpretI64,
    // Saturating Float-to-Int Conversions
    I32TruncSatF32S, I32TruncSatF32U, I32TruncSatF64S, I32TruncSatF64U,
    I64TruncSatF32S, I64TruncSatF32U, I64TruncSatF64S, I64TruncSatF64U,

    // > Table Instructions
    TableInit(ElemAddr, TableAddr),
    TableGet(TableAddr),
    TableSet(TableAddr),
    TableCopy { from: TableAddr, to: TableAddr },
    TableGrow(TableAddr),
    TableSize(TableAddr),
    TableFill(TableAddr),

    // > Bulk Memory Instructions
    MemoryInit(MemAddr, DataAddr),
    MemoryCopy(MemAddr, MemAddr),
    MemoryFill(MemAddr),
    DataDrop(DataAddr),
    ElemDrop(ElemAddr),

    // > SIMD Instructions
    Simd(SimdInstruction),
}

impl From<SimdInstruction> for Instruction {
    fn from(instr: SimdInstruction) -> Self {
        Instruction::Simd(instr)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "archive", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
#[rustfmt::skip] 
pub enum SimdInstruction {
    V128Load(MemoryArg),
    V128Load8x8S(MemoryArg), V128Load8x8U(MemoryArg),
    V128Load16x4S(MemoryArg), V128Load16x4U(MemoryArg),
    V128Load32x2S(MemoryArg), V128Load32x2U(MemoryArg),

    V128Load8Splat(MemoryArg), V128Load16Splat(MemoryArg), V128Load32Splat(MemoryArg), V128Load64Splat(MemoryArg),
    V128Load8Lane(MemoryArg, u8), V128Load16Lane(MemoryArg, u8), V128Load32Lane(MemoryArg, u8), V128Load64Lane(MemoryArg, u8),
    
    V128Load32Zero(MemoryArg), V128Load64Zero(MemoryArg),

    V128Store(MemoryArg), V128Store8Lane(MemoryArg, u8), V128Store16Lane(MemoryArg, u8), V128Store32Lane(MemoryArg, u8), V128Store64Lane(MemoryArg, u8),

    I8x16Shuffle(ConstIdx),
    V128Const(ConstIdx),

    I8x16ExtractLaneS(u8), I8x16ExtractLaneU(u8), I8x16ReplaceLane(u8),
    I16x8ExtractLaneS(u8), I16x8ExtractLaneU(u8), I16x8ReplaceLane(u8),
    I32x4ExtractLane(u8), I32x4ReplaceLane(u8),
    I64x2ExtractLane(u8), I64x2ReplaceLane(u8),
    F32x4ExtractLane(u8), F32x4ReplaceLane(u8),
    F64x2ExtractLane(u8), F64x2ReplaceLane(u8),

    V128Not, V128And, V128AndNot, V128Or, V128Xor, V128Bitselect, V128AnyTrue,

    I8x16Splat, I8x16Swizzle, I8x16Eq, I8x16Ne, I8x16LtS, I8x16LtU, I8x16GtS, I8x16GtU, I8x16LeS, I8x16LeU, I8x16GeS, I8x16GeU,
    I16x8Splat, I16x8Eq, I16x8Ne, I16x8LtS, I16x8LtU, I16x8GtS, I16x8GtU, I16x8LeS, I16x8LeU, I16x8GeS, I16x8GeU,
    I32x4Splat, I32x4Eq, I32x4Ne, I32x4LtS, I32x4LtU, I32x4GtS, I32x4GtU, I32x4LeS, I32x4LeU, I32x4GeS, I32x4GeU,
    I64x2Splat, I64x2Eq, I64x2Ne, I64x2LtS, I64x2GtS, I64x2LeS, I64x2GeS, 
    F32x4Splat, F32x4Eq, F32x4Ne, F32x4Lt, F32x4Gt, F32x4Le, F32x4Ge,
    F64x2Splat, F64x2Eq, F64x2Ne, F64x2Lt, F64x2Gt, F64x2Le, F64x2Ge,

    I8x16Abs, I8x16Neg, I8x16AllTrue, I8x16Bitmask, I8x16Shl, I8x16ShrS, I8x16ShrU, I8x16Add, I8x16Sub, I8x16MinS, I8x16MinU, I8x16MaxS, I8x16MaxU,
    I16x8Abs, I16x8Neg, I16x8AllTrue, I16x8Bitmask, I16x8Shl, I16x8ShrS, I16x8ShrU, I16x8Add, I16x8Sub, I16x8MinS, I16x8MinU, I16x8MaxS, I16x8MaxU,
    I32x4Abs, I32x4Neg, I32x4AllTrue, I32x4Bitmask, I32x4Shl, I32x4ShrS, I32x4ShrU, I32x4Add, I32x4Sub, I32x4MinS, I32x4MinU, I32x4MaxS, I32x4MaxU, 
    I64x2Abs, I64x2Neg, I64x2AllTrue, I64x2Bitmask, I64x2Shl, I64x2ShrS, I64x2ShrU, I64x2Add, I64x2Sub, I64x2Mul,

    I8x16NarrowI16x8S, I8x16NarrowI16x8U, I8x16AddSatS, I8x16AddSatU, I8x16SubSatS, I8x16SubSatU, I8x16AvgrU,
    I16x8NarrowI32x4S, I16x8NarrowI32x4U, I16x8AddSatS, I16x8AddSatU, I16x8SubSatS, I16x8SubSatU, I16x8AvgrU,

    I16x8ExtAddPairwiseI8x16S, I16x8ExtAddPairwiseI8x16U, I16x8Mul,
    I32x4ExtAddPairwiseI16x8S, I32x4ExtAddPairwiseI16x8U, I32x4Mul,

    I16x8ExtMulLowI8x16S, I16x8ExtMulLowI8x16U, I16x8ExtMulHighI8x16S, I16x8ExtMulHighI8x16U,
    I32x4ExtMulLowI16x8S, I32x4ExtMulLowI16x8U, I32x4ExtMulHighI16x8S, I32x4ExtMulHighI16x8U,
    I64x2ExtMulLowI32x4S, I64x2ExtMulLowI32x4U, I64x2ExtMulHighI32x4S, I64x2ExtMulHighI32x4U,

    I16x8ExtendLowI8x16S, I16x8ExtendLowI8x16U, I16x8ExtendHighI8x16S, I16x8ExtendHighI8x16U,
    I32x4ExtendLowI16x8S, I32x4ExtendLowI16x8U, I32x4ExtendHighI16x8S, I32x4ExtendHighI16x8U,
    I64x2ExtendLowI32x4S, I64x2ExtendLowI32x4U, I64x2ExtendHighI32x4S, I64x2ExtendHighI32x4U,

    I8x16Popcnt, I16x8Q15MulrSatS, I32x4DotI16x8S,

    F32x4Ceil, F32x4Floor, F32x4Trunc, F32x4Nearest, F32x4Abs, F32x4Neg, F32x4Sqrt, F32x4Add, F32x4Sub, F32x4Mul, F32x4Div, F32x4Min, F32x4Max, F32x4PMin, F32x4PMax,
    F64x2Ceil, F64x2Floor, F64x2Trunc, F64x2Nearest, F64x2Abs, F64x2Neg, F64x2Sqrt, F64x2Add, F64x2Sub, F64x2Mul, F64x2Div, F64x2Min, F64x2Max, F64x2PMin, F64x2PMax,
    I32x4TruncSatF32x4S, I32x4TruncSatF32x4U,
    F32x4ConvertI32x4S, F32x4ConvertI32x4U,
    I32x4TruncSatF64x2SZero, I32x4TruncSatF64x2UZero,
    F64x2ConvertLowI32x4S, F64x2ConvertLowI32x4U,
    F32x4DemoteF64x2Zero, F64x2PromoteLowF32x4,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "archive", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
#[rustfmt::skip]
pub enum RelaxedSimd {
    I8x16RelaxedSwizzle,
    I32x4RelaxedTruncF32x4S, I32x4RelaxedTruncF32x4U,
    I32x4RelaxedTruncF64x2SZero, I32x4RelaxedTruncF64x2UZero,
    F32x4RelaxedMadd, F32x4RelaxedNmadd,
    F64x2RelaxedMadd, F64x2RelaxedNmadd,
    I8x16RelaxedLaneselect,
    I16x8RelaxedLaneselect,
    I32x4RelaxedLaneselect,
    I64x2RelaxedLaneselect,
    F32x4RelaxedMin, F32x4RelaxedMax,
    F64x2RelaxedMin, F64x2RelaxedMax,
    I16x8RelaxedQ15mulrS,
    I16x8RelaxedDotI8x16I7x16S,
    I32x4RelaxedDotI8x16I7x16AddS
}
