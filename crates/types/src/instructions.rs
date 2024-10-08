use super::{FuncAddr, GlobalAddr, LabelAddr, LocalAddr, TableAddr, TypeAddr, ValType};
use crate::{DataAddr, ElemAddr, MemAddr};

/// Represents a memory immediate in a WebAssembly memory instruction.
#[derive(Debug, Copy, Clone, PartialEq)]
#[cfg_attr(feature = "archive", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
pub struct MemoryArg {
    pub offset: u64,
    pub mem_addr: MemAddr,
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
    LocalCopy32(LocalAddr, LocalAddr), LocalCopy64(LocalAddr, LocalAddr), LocalCopy128(LocalAddr, LocalAddr), LocalCopy128Ref(LocalAddr, LocalAddr), LocalCopyRef(LocalAddr, LocalAddr),
    LocalsStore32(LocalAddr, LocalAddr, u32, MemAddr), LocalsStore64(LocalAddr, LocalAddr, u32, MemAddr), LocalsStore128(LocalAddr, LocalAddr, u32, MemAddr), LocalsStoreRef(LocalAddr, LocalAddr, u32, MemAddr),

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
    ReturnCall(FuncAddr),
    ReturnCallIndirect(TypeAddr, TableAddr),
 
    // > Parametric Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#parametric-instructions>
    Drop32,
    Drop64,
    Drop128,
    DropRef,

    Select32,
    Select64,
    Select128,
    SelectRef,

    // > Variable Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#variable-instructions>
    LocalGet32(LocalAddr),
    LocalGet64(LocalAddr),
    LocalGet128(LocalAddr),
    LocalGetRef(LocalAddr),

    LocalSet32(LocalAddr),
    LocalSet64(LocalAddr),
    LocalSet128(LocalAddr),
    LocalSetRef(LocalAddr),

    LocalTee32(LocalAddr),
    LocalTee64(LocalAddr),
    LocalTee128(LocalAddr),
    LocalTeeRef(LocalAddr),

    GlobalGet(GlobalAddr),
    GlobalSet32(GlobalAddr),
    GlobalSet64(GlobalAddr),
    GlobalSet128(GlobalAddr),
    GlobalSetRef(GlobalAddr),

    // > Memory Instructions
    I32Load { offset: u64, mem_addr: MemAddr },
    I64Load { offset: u64, mem_addr: MemAddr },
    F32Load { offset: u64, mem_addr: MemAddr },
    F64Load { offset: u64, mem_addr: MemAddr },
    I32Load8S { offset: u64, mem_addr: MemAddr },
    I32Load8U { offset: u64, mem_addr: MemAddr },
    I32Load16S { offset: u64, mem_addr: MemAddr },
    I32Load16U { offset: u64, mem_addr: MemAddr },
    I64Load8S { offset: u64, mem_addr: MemAddr },
    I64Load8U { offset: u64, mem_addr: MemAddr },
    I64Load16S { offset: u64, mem_addr: MemAddr },
    I64Load16U { offset: u64, mem_addr: MemAddr },
    I64Load32S { offset: u64, mem_addr: MemAddr },
    I64Load32U { offset: u64, mem_addr: MemAddr },
    I32Store { offset: u64, mem_addr: MemAddr },
    I64Store { offset: u64, mem_addr: MemAddr },
    F32Store { offset: u64, mem_addr: MemAddr },
    F64Store { offset: u64, mem_addr: MemAddr },
    I32Store8 { offset: u64, mem_addr: MemAddr },
    I32Store16 { offset: u64, mem_addr: MemAddr },
    I64Store8 { offset: u64, mem_addr: MemAddr },
    I64Store16 { offset: u64, mem_addr: MemAddr },
    I64Store32 { offset: u64, mem_addr: MemAddr },
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

    // // > SIMD Instructions
    // V128Load(MemoryArg), V128Load8x8S { offset: u64, mem_addr: MemAddr }, V128Load8x8U { offset: u64, mem_addr: MemAddr }, V128Load16x4S { offset: u64, mem_addr: MemAddr }, V128Load16x4U { offset: u64, mem_addr: MemAddr }, V128Load32x2S { offset: u64, mem_addr: MemAddr }, V128Load32x2U { offset: u64, mem_addr: MemAddr }, V128Load8Splat { offset: u64, mem_addr: MemAddr }, V128Load16Splat { offset: u64, mem_addr: MemAddr }, V128Load32Splat { offset: u64, mem_addr: MemAddr }, V128Load64Splat { offset: u64, mem_addr: MemAddr }, V128Load32Zero { offset: u64, mem_addr: MemAddr }, V128Load64Zero { offset: u64, mem_addr: MemAddr }, 
    // V128Store { offset: u64, mem_addr: MemAddr }, V128Store8x8 { offset: u64, mem_addr: MemAddr }, V128Store16x4 { offset: u64, mem_addr: MemAddr }, V128Store32x2 { offset: u64, mem_addr: MemAddr },
}
