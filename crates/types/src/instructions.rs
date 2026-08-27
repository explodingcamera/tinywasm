use alloc::boxed::Box;

use super::{FuncAddr, GlobalAddr, LocalAddr, TableAddr, TagAddr, TypeAddr, ValueCounts};
use crate::operands::{read_field, write_field};
use crate::{
    DataAddr, ElemAddr, MemAddr, ModuleFuncIdx, Operand64, Operand64Idx, Operand128, Operand128Idx, OperandIdx, RefType,
};

/// Identifies the packed full-width memory operand layout.
pub enum MemoryOperand {}

/// Identifies the packed compact memory operand layout.
pub enum CompactMemoryOperand {}

/// Identifies a packed branch-table operand.
pub enum BranchTableOperand {}

/// Identifies a packed constant memory-fill operand.
pub enum MemoryFillOperand {}

/// Identifies a packed local update and branch operand.
pub enum LocalUpdateOperand {}

/// Identifies a packed global update and branch operand.
pub enum GlobalUpdateOperand {}

/// Identifies a packed local update, comparison, and branch operand.
pub enum LocalUpdateCmpOperand {}

/// A compact memory immediate used by optimized instructions.
///
/// Optimized instructions use this representation when the offset and
/// module-local memory index fit its narrower fields.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct CompactMemoryArg {
    offset: u32,
    mem_addr: u16,
}

/// An indexed memory argument and up to two local operands.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct MemoryLocalArg {
    pub memory_arg_idx: Operand64Idx<CompactMemoryOperand>,
    pub local1: u8,
    pub local2: u8,
}

/// An indexed memory argument and SIMD lane.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct MemoryLaneArg {
    pub memory_arg_idx: Operand128Idx<MemoryOperand>,
    pub lane: u8,
}

/// A 32-bit immediate and local operand that fit in an instruction payload.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct I32LocalArg {
    pub value: i32,
    pub local: LocalAddr,
}

/// Three local operands that fit in an instruction payload.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct LocalTripleArg {
    pub left: LocalAddr,
    pub right: LocalAddr,
    pub dst: LocalAddr,
}

/// A branch target and local operand that fit in an instruction payload.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct TargetLocalArg {
    pub target_ip: u32,
    pub local: LocalAddr,
}

/// An operation packed inline with an indexed operand.
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "archive",
    serde(bound(serialize = "Op: Copy + serde::Serialize", deserialize = "Op: Copy + serde::de::DeserializeOwned",))
)]
#[repr(Rust, packed)]
pub struct PackedOp<Op, T, const WIDTH: usize> {
    pub op: Op,
    pub index: OperandIdx<T, WIDTH>,
}

/// An operation packed with an index into the 64-bit operand lane.
pub type PackedOp64<Op, T> = PackedOp<Op, T, 8>;

/// An operation packed with an index into the 128-bit operand lane.
pub type PackedOp128<Op, T> = PackedOp<Op, T, 16>;

impl<Op: Copy, T, const WIDTH: usize> Copy for PackedOp<Op, T, WIDTH> {}

impl<Op: Copy, T, const WIDTH: usize> Clone for PackedOp<Op, T, WIDTH> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Op: Copy + PartialEq, T, const WIDTH: usize> PartialEq for PackedOp<Op, T, WIDTH> {
    fn eq(&self, other: &Self) -> bool {
        let (op, index) = (self.op, self.index);
        let (other_op, other_index) = (other.op, other.index);
        op == other_op && index == other_index
    }
}

impl<Op: Copy + Eq, T, const WIDTH: usize> Eq for PackedOp<Op, T, WIDTH> {}

#[cfg(feature = "debug")]
impl<Op: Copy + core::fmt::Debug, T, const WIDTH: usize> core::fmt::Debug for PackedOp<Op, T, WIDTH> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let (op, index) = (self.op, self.index);
        formatter.debug_struct("PackedOp").field("op", &op).field("index", &index).finish()
    }
}

impl<Op, T, const WIDTH: usize> PackedOp<Op, T, WIDTH> {
    /// Creates a packed operation from an operator and operand index.
    #[inline]
    pub const fn new(op: Op, index: OperandIdx<T, WIDTH>) -> Self {
        Self { op, index }
    }
}

/// A catch clause attached to a lowered `try_table` instruction.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ExceptionCatch {
    /// Catch exceptions carrying the specified module-local tag.
    Tag { tag: TagAddr, landing_pad: u32, base: ValueCounts, with_ref: bool },
    /// Catch any exception.
    All { landing_pad: u32, base: ValueCounts, with_ref: bool },
}

impl ExceptionCatch {
    /// Returns the catch landing pad instruction pointer.
    pub const fn landing_pad(self) -> u32 {
        match self {
            Self::Tag { landing_pad, .. } | Self::All { landing_pad, .. } => landing_pad,
        }
    }

    /// Returns whether this clause exposes the caught exception reference.
    pub const fn with_ref(self) -> bool {
        match self {
            Self::Tag { with_ref, .. } | Self::All { with_ref, .. } => with_ref,
        }
    }
}

/// A statically lowered exception handler range.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ExceptionHandler {
    /// First protected instruction, inclusive.
    pub start_ip: u32,
    /// End of the protected instruction range, exclusive.
    pub end_ip: u32,
    /// Catch clauses in source order.
    pub catches: Box<[ExceptionCatch]>,
}

impl Operand128<MemoryOperand> {
    /// Creates a full-width memory operand.
    #[inline]
    pub fn new(offset: u64, memory: MemAddr) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u64, 0, offset);
        write_field!(operand, u32, 8, memory);
        operand
    }

    /// Returns the static byte offset.
    #[inline(always)]
    pub fn offset(&self) -> u64 {
        read_field!(self, u64, 0)
    }

    /// Returns the module-local memory index.
    #[inline(always)]
    pub fn memory(&self) -> MemAddr {
        read_field!(self, u32, 8)
    }
}

impl Operand64<CompactMemoryOperand> {
    /// Creates a compact memory operand.
    #[inline]
    pub fn new(offset: u32, memory: u16) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u32, 0, offset);
        write_field!(operand, u16, 4, memory);
        operand
    }

    /// Returns the static byte offset.
    #[inline(always)]
    pub fn offset(&self) -> u32 {
        read_field!(self, u32, 0)
    }

    /// Returns the module-local memory index.
    #[inline(always)]
    pub fn memory(&self) -> u16 {
        read_field!(self, u16, 4)
    }
}

impl Operand128<BranchTableOperand> {
    #[inline]
    pub fn new(target: u32, start: u32, len: u32) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u32, 0, target);
        write_field!(operand, u32, 4, start);
        write_field!(operand, u32, 8, len);
        operand
    }
    #[inline(always)]
    pub fn start(&self) -> u32 {
        read_field!(self, u32, 4)
    }
    #[inline(always)]
    pub fn size(&self) -> u32 {
        read_field!(self, u32, 8)
    }
}

impl Operand128<MemoryFillOperand> {
    #[inline]
    pub fn new(memory: MemAddr, byte: u8, value: i32) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u32, 0, memory);
        write_field!(operand, u8, 4, byte);
        write_field!(operand, i32, 5, value);
        operand
    }
    #[inline(always)]
    pub fn memory(&self) -> MemAddr {
        read_field!(self, u32, 0)
    }
    #[inline(always)]
    pub fn byte(&self) -> u8 {
        read_field!(self, u8, 4)
    }
    #[inline(always)]
    pub fn value(&self) -> i32 {
        read_field!(self, i32, 5)
    }
}

impl Operand128<LocalUpdateOperand> {
    #[inline]
    pub fn new(target: u32, value: i32, local: LocalAddr, on_zero: bool) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u32, 0, target);
        write_field!(operand, i32, 4, value);
        write_field!(operand, u16, 8, local);
        write_field!(operand, u8, 10, u8::from(on_zero));
        operand
    }
    #[inline(always)]
    pub fn value(&self) -> i32 {
        read_field!(self, i32, 4)
    }
    #[inline(always)]
    pub fn local(&self) -> LocalAddr {
        read_field!(self, u16, 8)
    }
    #[inline(always)]
    pub fn on_zero(&self) -> bool {
        read_field!(self, u8, 10) != 0
    }
}

impl Operand128<GlobalUpdateOperand> {
    #[inline]
    pub fn new(target: u32, value: i32, global: GlobalAddr, on_zero: bool) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u32, 0, target);
        write_field!(operand, i32, 4, value);
        write_field!(operand, u32, 8, global);
        write_field!(operand, u8, 12, u8::from(on_zero));
        operand
    }
    #[inline(always)]
    pub fn value(&self) -> i32 {
        read_field!(self, i32, 4)
    }
    #[inline(always)]
    pub fn global(&self) -> GlobalAddr {
        read_field!(self, u32, 8)
    }
    #[inline(always)]
    pub fn on_zero(&self) -> bool {
        read_field!(self, u8, 12) != 0
    }
}

impl Operand128<LocalUpdateCmpOperand> {
    #[inline]
    pub fn new(target: u32, value: i32, local: LocalAddr, right: LocalAddr) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u32, 0, target);
        write_field!(operand, i32, 4, value);
        write_field!(operand, u16, 8, local);
        write_field!(operand, u16, 10, right);
        operand
    }
    #[inline(always)]
    pub fn value(&self) -> i32 {
        read_field!(self, i32, 4)
    }
    #[inline(always)]
    pub fn local(&self) -> LocalAddr {
        read_field!(self, u16, 8)
    }
    #[inline(always)]
    pub fn right(&self) -> LocalAddr {
        read_field!(self, u16, 10)
    }
}

impl CompactMemoryArg {
    /// Returns the static byte offset.
    #[inline]
    pub const fn offset(self) -> u64 {
        self.offset as u64
    }

    /// Returns the module-local memory index.
    #[inline]
    pub const fn mem_addr(self) -> MemAddr {
        self.mem_addr as MemAddr
    }
}

impl TryFrom<Operand128<MemoryOperand>> for CompactMemoryArg {
    type Error = ();

    #[inline]
    fn try_from(arg: Operand128<MemoryOperand>) -> Result<Self, Self::Error> {
        Ok(Self {
            offset: u32::try_from(arg.offset()).map_err(|_| ())?,
            mem_addr: u16::try_from(arg.memory()).map_err(|_| ())?,
        })
    }
}

impl From<CompactMemoryArg> for Operand64<CompactMemoryOperand> {
    #[inline]
    fn from(arg: CompactMemoryArg) -> Self {
        Self::new(arg.offset, arg.mem_addr)
    }
}

const _: () = {
    assert!(core::mem::size_of::<Operand64Idx<i64>>() == 4);
    assert!(core::mem::size_of::<Operand64>() == 8);
    assert!(core::mem::size_of::<Operand128>() == 16);
    assert!(core::mem::size_of::<CompactMemoryArg>() == 6);
    assert!(core::mem::size_of::<MemoryLocalArg>() == 6);
    assert!(core::mem::size_of::<LocalTripleArg>() == 6);
};

#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ConstInstruction {
    I32Const(i32),
    I64Const(i64),
    F32Const(f32),
    F64Const(f64),
    V128Const([u8; 16]),
    GlobalGet32(GlobalAddr),
    GlobalGet64(GlobalAddr),
    GlobalGet128(GlobalAddr),
    GlobalGetRef(GlobalAddr),
    RefNull(RefType),
    RefFunc(ModuleFuncIdx),
    RefI31,
    AnyConvertExtern,
    ExternConvertAny,
    StructNew(TypeAddr),
    StructNewDefault(TypeAddr),
    ArrayNew(TypeAddr),
    ArrayNewDefault(TypeAddr),
    ArrayNewFixed(TypeAddr, u32),
    I32Add,
    I32Sub,
    I32Mul,
    I64Add,
    I64Sub,
    I64Mul,
}

/// An integer comparison operator, currently only used for conditional jumps.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum CmpOp {
    Eq,
    Ne,
    LtS,
    LtU,
    GtS,
    GtU,
    LeS,
    LeU,
    GeS,
    GeU,
}

impl CmpOp {
    /// Returns the comparison that is true exactly when `self` is false.
    #[inline]
    pub const fn inverse(self) -> Self {
        match self {
            Self::Eq => Self::Ne,
            Self::Ne => Self::Eq,
            Self::LtS => Self::GeS,
            Self::LtU => Self::GeU,
            Self::GtS => Self::LeS,
            Self::GtU => Self::LeU,
            Self::LeS => Self::GtS,
            Self::LeU => Self::GtU,
            Self::GeS => Self::LtS,
            Self::GeU => Self::LtU,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum BinOp {
    IAdd,
    ISub,
    IMul,
    IAnd,
    IOr,
    IXor,
    IShl,
    IShrS,
    IShrU,
    IRotl,
    IRotr,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FMin,
    FMax,
    FCopysign,
}

impl BinOp {
    /// Returns the update delta for `local OP immediate`, when it is a simple increment.
    #[inline]
    pub const fn inc_delta(self, immediate: i32) -> Option<i32> {
        match self {
            Self::IAdd => Some(immediate),
            Self::ISub => Some(immediate.wrapping_neg()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum BinOp128 {
    And,
    AndNot,
    Or,
    Xor,
    I64x2Add,
    I64x2Mul,
}

/// A TinyWasm bytecode instruction.
///
/// These instructions are an internal, version-specific representation and do not
/// map one-to-one to WebAssembly instructions. Their variants and serialized form
/// may change between TinyWasm releases.
#[rustfmt::skip]
#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum Instruction {
    LocalCopy32(LocalAddr, LocalAddr), LocalCopy64(LocalAddr, LocalAddr), LocalCopy128(LocalAddr, LocalAddr),
    AddConst32(i32), AddConst64(Operand64Idx<i64>),
    IncLocal32(I32LocalArg), IncLocal64(PackedOp64<LocalAddr, i64>),
    // The 32/64 suffix describes the operand width. Future compare-style ops may still yield i32 results.
    BinOpLocalLocal32(BinOp, LocalAddr, LocalAddr), BinOpLocalLocal64(BinOp, LocalAddr, LocalAddr),
    BinOpLocalLocal128(BinOp128, LocalAddr, LocalAddr),
    CmpLocalLocal32(CmpOp, LocalAddr, LocalAddr), CmpLocalLocal64(CmpOp, LocalAddr, LocalAddr),
    AddLocalLocalSet32(LocalTripleArg), AddLocalLocalTee32(LocalTripleArg),
    BinOpLocalLocalSet32(PackedOp64<BinOp, (u16, u16, u16)>), BinOpLocalLocalSet64(PackedOp64<BinOp, (u16, u16, u16)>), BinOpLocalLocalSet128(PackedOp64<BinOp128, (u16, u16, u16)>),
    BinOpLocalLocalTee32(PackedOp64<BinOp, (u16, u16, u16)>), BinOpLocalLocalTee64(PackedOp64<BinOp, (u16, u16, u16)>), BinOpLocalLocalTee128(PackedOp64<BinOp128, (u16, u16, u16)>),
    AddLocalConst32(I32LocalArg), SubLocalConst32(I32LocalArg), MulLocalConst32(I32LocalArg),
    BinOpLocalConst32(PackedOp64<BinOp, (u16, u32)>), BinOpLocalConst64(PackedOp128<BinOp, (u16, u64)>),
    BinOpGlobalConst32(PackedOp64<BinOp, (u32, u32)>), BinOpGlobalConst64(PackedOp128<BinOp, (u32, u64)>),
    BinOpGlobalConst128(PackedOp64<BinOp128, (u32, Operand128Idx<[u8; 16]>)>), BinOpLocalConst128(PackedOp64<BinOp128, (u16, Operand128Idx<[u8; 16]>)>),
    BinOpLocalConstSet32(PackedOp64<BinOp, (u16, u16, u32)>), BinOpLocalConstSet64(PackedOp128<BinOp, (u16, u16, u64)>), BinOpLocalConstSet128(PackedOp64<BinOp128, (u16, u16, Operand128Idx<[u8; 16]>)>),
    BinOpLocalConstTee32(PackedOp64<BinOp, (u16, u16, u32)>), BinOpLocalConstTee64(PackedOp128<BinOp, (u16, u16, u64)>), BinOpLocalConstTee128(PackedOp64<BinOp128, (u16, u16, Operand128Idx<[u8; 16]>)>),
    BinOpStackLocal32(BinOp, LocalAddr),
    BinOpStackLocalSet32(BinOp, LocalAddr, LocalAddr),
    BinOpStackLocalTee32(BinOp, LocalAddr, LocalAddr),
    BinOpStackLocal128(BinOp128, LocalAddr),
    BinOpStackGlobal32(BinOp, u32),
    BinOpStackGlobal64(BinOp, u32),
    SetLocalConst32(I32LocalArg), SetLocalConst64(PackedOp64<LocalAddr, i64>), SetLocalConst128(PackedOp128<LocalAddr, [u8; 16]>),
    IncMemoryLocal32(MemoryLocalArg), IncMemoryLocal64(MemoryLocalArg),
    StoreLocalLocal32(MemoryLocalArg), StoreLocalLocal64(MemoryLocalArg), StoreLocalLocal128(MemoryLocalArg),
    LoadLocal32(MemoryLocalArg), LoadLocal64(MemoryLocalArg),
    LoadLocal8S32(MemoryLocalArg), LoadLocal8U32(MemoryLocalArg),
    LoadLocal16S32(MemoryLocalArg), LoadLocal16U32(MemoryLocalArg),
    LoadLocalTee32(MemoryLocalArg), LoadLocalSet32(MemoryLocalArg),
    LoadLocalTee8S32(MemoryLocalArg), LoadLocalTee8U32(MemoryLocalArg),
    LoadLocalTee16S32(MemoryLocalArg), LoadLocalTee16U32(MemoryLocalArg),
    LoadLocalSet8S32(MemoryLocalArg), LoadLocalSet8U32(MemoryLocalArg),
    LoadLocalSet16S32(MemoryLocalArg), LoadLocalSet16U32(MemoryLocalArg),
    LoadLocalTee128(MemoryLocalArg), LoadLocalSet128(MemoryLocalArg),
    AndConstTee32(I32LocalArg), SubConstTee32(I32LocalArg),
    AndConstTee64(PackedOp64<LocalAddr, i64>), SubConstTee64(PackedOp64<LocalAddr, i64>),
    MulAccLocal32(LocalAddr), MulAccLocal64(LocalAddr),
    FMulAccLocal32(LocalAddr), FMulAccLocal64(LocalAddr),
    I32Add3,
    I64Add3,
    FMaStoreF32(CompactMemoryArg),
    FMaStoreF64(CompactMemoryArg),

    // > Control Instructions (jump-oriented, lowered from structured control during parsing)
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#control-instructions>
    Unreachable,
    Jump(u32),
    JumpIfZero32(u32),
    JumpIfNonZero32(u32),
    JumpIfZero64(u32),
    JumpIfNonZero64(u32),
    JumpIfRefNull(u32),
    JumpIfRefNonNull(u32),
    JumpIfLocalZero32(TargetLocalArg), JumpIfLocalNonZero32(TargetLocalArg),
    JumpIfLocalZero64(TargetLocalArg), JumpIfLocalNonZero64(TargetLocalArg),
    JumpCmpStackConst32(PackedOp64<CmpOp, (u32, i32)>), JumpCmpStackConst64(PackedOp128<CmpOp, (u32, i64)>),
    JumpCmpStackLocal32(PackedOp64<CmpOp, (u32, u16)>), JumpCmpStackLocal64(PackedOp64<CmpOp, (u32, u16)>),
    BinOpLocalConstJump32(PackedOp128<BinOp, LocalUpdateOperand>), BinOpLocalConstJumpCmpLocal32(PackedOp128<(BinOp, CmpOp), LocalUpdateCmpOperand>),
    BinOpStackConstTeeLocalJump32(PackedOp128<BinOp, LocalUpdateOperand>), BinOpGlobalConstJump32(PackedOp128<BinOp, GlobalUpdateOperand>),
    IncLocalJump32(Operand128Idx<LocalUpdateOperand>), IncStackTeeLocalJump32(Operand128Idx<LocalUpdateOperand>), IncGlobalJump32(Operand128Idx<GlobalUpdateOperand>),
    IncLocalJumpCmpLocal32(PackedOp128<CmpOp, LocalUpdateCmpOperand>), JumpCmpLocalConst32(PackedOp128<CmpOp, (u32, i32, u16)>), JumpCmpLocalConst64(PackedOp128<CmpOp, (u32, i32, u16)>),
    JumpCmpLocalLocal32(PackedOp64<CmpOp, (u32, u16, u16)>), JumpCmpLocalLocal64(PackedOp64<CmpOp, (u32, u16, u16)>),
    DropKeep32 { base: u16, keep: u16 }, DropKeep64 { base: u16, keep: u16 }, DropKeep128 { base: u16, keep: u16 }, BranchTable(Operand128Idx<BranchTableOperand>),
    Return,
    ReturnVoid,
    Return32,
    Return64,
    Return128,
    Call(FuncAddr),
    CallSelf,
    CallIndirect(Operand64Idx<(u32, u32)>),
    CallRef(TypeAddr),
    ReturnCall(FuncAddr),
    ReturnCallSelf,
    ReturnCallIndirect(Operand64Idx<(u32, u32)>),
    ReturnCallRef(TypeAddr),
    Throw(TagAddr),
    ThrowRef,

    // > Parametric Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#parametric-instructions>
    Drop32, Select32,
    Drop64, Select64,
    Drop128, Select128,
    SelectMulti(ValueCounts),

    // > Variable Instructions
    // See <https://webassembly.github.io/spec/core/binary/instructions.html#variable-instructions>
    GlobalGet32(GlobalAddr), GlobalSet32(GlobalAddr), GlobalTee32(GlobalAddr), LocalGet32(LocalAddr), LocalSet32(LocalAddr), LocalTee32(LocalAddr),
    GlobalGet64(GlobalAddr), GlobalSet64(GlobalAddr), GlobalTee64(GlobalAddr), LocalGet64(LocalAddr), LocalSet64(LocalAddr), LocalTee64(LocalAddr),
    GlobalGet128(GlobalAddr), GlobalSet128(GlobalAddr), GlobalTee128(GlobalAddr), LocalGet128(LocalAddr), LocalSet128(LocalAddr), LocalTee128(LocalAddr),

    // > Memory Instructions
    I32Load(Operand128Idx<MemoryOperand>), I64Load(Operand128Idx<MemoryOperand>), F32Load(Operand128Idx<MemoryOperand>), F64Load(Operand128Idx<MemoryOperand>),
    I32Load8S(Operand128Idx<MemoryOperand>), I32Load8U(Operand128Idx<MemoryOperand>), I32Load16S(Operand128Idx<MemoryOperand>), I32Load16U(Operand128Idx<MemoryOperand>),
    I64Load8S(Operand128Idx<MemoryOperand>), I64Load8U(Operand128Idx<MemoryOperand>), I64Load16S(Operand128Idx<MemoryOperand>), I64Load16U(Operand128Idx<MemoryOperand>), I64Load32S(Operand128Idx<MemoryOperand>), I64Load32U(Operand128Idx<MemoryOperand>),
    I32Store(Operand128Idx<MemoryOperand>), I64Store(Operand128Idx<MemoryOperand>), F32Store(Operand128Idx<MemoryOperand>), F64Store(Operand128Idx<MemoryOperand>),
    I32Store8(Operand128Idx<MemoryOperand>), I32Store16(Operand128Idx<MemoryOperand>), I64Store8(Operand128Idx<MemoryOperand>), I64Store16(Operand128Idx<MemoryOperand>), I64Store32(Operand128Idx<MemoryOperand>),
    MemorySize(MemAddr),
    MemoryGrow(MemAddr),

    // > Constants
    Const32(i32),
    Const64Imm(i32),
    Const64(Operand64Idx<i64>),

    // > Reference Types
    RefNull(RefType),
    RefFunc(FuncAddr),
    RefIsNull,
    RefAsNonNull,
    RefI31,
    I31GetS,
    I31GetU,
    RefEq,
    RefTest(RefType),
    RefCast(RefType),
    BrOnCast(Operand64Idx<(u32, u32)>),
    BrOnCastFail(Operand64Idx<(u32, u32)>),

    // > GC Objects
    StructNew(TypeAddr),
    StructNewDefault(TypeAddr),
    StructGet(Operand64Idx<(u32, u32)>), StructGetS(Operand64Idx<(u32, u32)>), StructGetU(Operand64Idx<(u32, u32)>), StructSet(Operand64Idx<(u32, u32)>),
    ArrayNew(TypeAddr),
    ArrayNewDefault(TypeAddr),
    ArrayNewFixed(Operand64Idx<(u32, u32)>),
    ArrayNewData(Operand64Idx<(u32, u32)>), ArrayNewElem(Operand64Idx<(u32, u32)>),
    ArrayGet(TypeAddr),
    ArrayGetS(TypeAddr),
    ArrayGetU(TypeAddr),
    ArraySet(TypeAddr),
    ArrayLen,
    ArrayFill(TypeAddr),
    ArrayCopy(Operand64Idx<(u32, u32)>), ArrayInitData(Operand64Idx<(u32, u32)>), ArrayInitElem(Operand64Idx<(u32, u32)>),

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

    // Saturating Float-to-Int Conversions
    I32TruncSatF32S, I32TruncSatF32U, I32TruncSatF64S, I32TruncSatF64U,
    I64TruncSatF32S, I64TruncSatF32U, I64TruncSatF64S, I64TruncSatF64U,

    // > Table Instructions
    TableInit(Operand64Idx<(u32, u32)>),
    TableGet(TableAddr),
    TableSet(TableAddr),
    TableCopy(Operand64Idx<(u32, u32)>),
    TableGrow(TableAddr),
    TableSize(TableAddr),
    TableFill(TableAddr),

    // > Bulk Memory Instructions
    MemoryInit(Operand64Idx<(u32, u32)>), MemoryCopy(Operand64Idx<(u32, u32)>),
    MemoryFill(MemAddr),
    MemoryFillConst(Operand128Idx<MemoryFillOperand>),
    DataDrop(DataAddr),
    ElemDrop(ElemAddr),

    // > Wide Arithmetic
    I64Add128, I64Sub128, I64MulWideS, I64MulWideU,

    // > SIMD
    V128Load(Operand128Idx<MemoryOperand>), V128Load8x8S(Operand128Idx<MemoryOperand>), V128Load8x8U(Operand128Idx<MemoryOperand>),
    V128Load16x4S(Operand128Idx<MemoryOperand>), V128Load16x4U(Operand128Idx<MemoryOperand>), V128Load32x2S(Operand128Idx<MemoryOperand>), V128Load32x2U(Operand128Idx<MemoryOperand>),

    V128Load8Splat(Operand128Idx<MemoryOperand>), V128Load16Splat(Operand128Idx<MemoryOperand>), V128Load32Splat(Operand128Idx<MemoryOperand>), V128Load64Splat(Operand128Idx<MemoryOperand>),
    V128Load8Lane(MemoryLaneArg), V128Load16Lane(MemoryLaneArg), V128Load32Lane(MemoryLaneArg), V128Load64Lane(MemoryLaneArg),

    V128Load32Zero(Operand128Idx<MemoryOperand>), V128Load64Zero(Operand128Idx<MemoryOperand>),

    V128Store(Operand128Idx<MemoryOperand>), V128Store8Lane(MemoryLaneArg), V128Store16Lane(MemoryLaneArg), V128Store32Lane(MemoryLaneArg), V128Store64Lane(MemoryLaneArg),

    I8x16Shuffle(Operand128Idx<[u8; 16]>),
    Const128Imm(u32), Const128(Operand128Idx<[u8; 16]>),

    I8x16ExtractLaneS(u8), I8x16ExtractLaneU(u8), I8x16ReplaceLane(u8),
    I16x8ExtractLaneS(u8), I16x8ExtractLaneU(u8), I16x8ReplaceLane(u8),
    I32x4ExtractLane(u8), I32x4ReplaceLane(u8),
    I64x2ExtractLane(u8), I64x2ReplaceLane(u8),
    F32x4ExtractLane(u8), F32x4ReplaceLane(u8),
    F64x2ExtractLane(u8), F64x2ReplaceLane(u8),

    V128Not, V128And, V128AndNot, V128Or, V128Xor, V128Bitselect, V128AnyTrue, I8x16Swizzle,
    I8x16Splat, I8x16Eq, I8x16Ne, I8x16LtS, I8x16LtU, I8x16GtS, I8x16GtU, I8x16LeS, I8x16LeU, I8x16GeS, I8x16GeU,
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

    // > Relaxed SIMD
    I8x16RelaxedSwizzle,
    I32x4RelaxedTruncF32x4S, I32x4RelaxedTruncF32x4U,
    I32x4RelaxedTruncF64x2SZero, I32x4RelaxedTruncF64x2UZero,
    F32x4RelaxedMadd, F32x4RelaxedNmadd,
    F64x2RelaxedMadd, F64x2RelaxedNmadd,
    I8x16RelaxedLaneselect, I16x8RelaxedLaneselect,
    I32x4RelaxedLaneselect, I64x2RelaxedLaneselect,
    F32x4RelaxedMin, F32x4RelaxedMax,
    F64x2RelaxedMin, F64x2RelaxedMax,
    I16x8RelaxedQ15mulrS,
    I16x8RelaxedDotI8x16I7x16S,
    I32x4RelaxedDotI8x16I7x16AddS,

    SelectStore32(Operand128Idx<MemoryOperand>), SelectStore64(Operand128Idx<MemoryOperand>),
}

const _: () = assert!(core::mem::size_of::<Instruction>() == 8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_instruction_layout_is_exact() {
        assert_eq!(core::mem::size_of::<Instruction>(), 8);
        assert_eq!(core::mem::size_of::<CompactMemoryArg>(), 6);
        assert_eq!(core::mem::size_of::<MemoryLocalArg>(), 6);
        assert_eq!(core::mem::size_of::<LocalTripleArg>(), 6);
    }

    #[test]
    fn operand_lanes_round_trip_max_values() {
        let memory = Operand128::<MemoryOperand>::new(u64::MAX, u32::MAX);
        assert_eq!(memory.offset(), u64::MAX);
        assert_eq!(memory.memory(), u32::MAX);

        let value = Operand128::<LocalUpdateOperand>::new(u32::MAX, i32::MIN, u16::MAX, true);
        assert_eq!(value.target(), u32::MAX);
        assert_eq!(value.value(), i32::MIN);
        assert_eq!(value.local(), u16::MAX);
        assert!(value.on_zero());
    }
}
