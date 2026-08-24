use alloc::boxed::Box;

use super::{FuncAddr, GlobalAddr, LocalAddr, TableAddr, TagAddr, TypeAddr, ValueCounts};
use crate::operands::sealed;
use crate::{DataAddr, ElemAddr, MemAddr, ModuleFuncIdx, Operand64, Operand128, OperandIdx, OperandType, RefType};

/// Represents a memory immediate in a WebAssembly memory instruction.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct MemoryArg {
    offset: u64,
    mem_addr: MemAddr,
}

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
    pub memory_arg_idx: OperandIdx<CompactMemoryArg>,
    pub local1: u8,
    pub local2: u8,
}

/// An indexed memory argument and SIMD lane.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct MemoryLaneArg {
    pub memory_arg_idx: OperandIdx<MemoryArg>,
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

impl sealed::Sealed for LocalTripleArg {}

impl OperandType for LocalTripleArg {
    type Raw = Operand64;

    #[inline(always)]
    fn decode(raw: Self::Raw) -> Self {
        Self { left: raw.u16::<0>(), right: raw.u16::<2>(), dst: raw.u16::<4>() }
    }

    #[inline]
    fn encode(self) -> Self::Raw {
        Operand64::default().with_u16::<0>(self.left).with_u16::<2>(self.right).with_u16::<4>(self.dst)
    }
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

macro_rules! operand_view {
    ($name:ident, $lane:ident, $raw:ty, { $($field:ident : $ty:ty = $read:ident / $write:ident ($offset:expr)),+ $(,)? }) => {
        #[derive(Copy, Clone, PartialEq, Eq)]
        #[cfg_attr(feature = "debug", derive(Debug))]
        pub struct $name {
            $(pub $field: $ty),+
        }

        impl sealed::Sealed for $name {}

        impl OperandType for $name {
            type Raw = $raw;

            #[inline(always)]
            fn decode(raw: Self::Raw) -> Self {
                Self { $($field: raw.$read::<$offset>() as $ty),+ }
            }

            #[inline]
            fn encode(self) -> Self::Raw {
                <$raw>::default()$(.$write::<$offset>(self.$field as _))+
            }
        }
    };
}

operand_view!(I64Operand, operands64, Operand64, { value: i64 = u64 / with_u64(0) });
operand_view!(TwoU32, operands64, Operand64, {
    first: u32 = u32 / with_u32(0), second: u32 = u32 / with_u32(4)
});
operand_view!(LocalU32, operands64, Operand64, {
    local: u16 = u16 / with_u16(0), value: u32 = u32 / with_u32(2)
});
operand_view!(TargetLocal, operands64, Operand64, {
    target: u32 = u32 / with_u32(0), local: u16 = u16 / with_u16(4)
});

operand_view!(LocalConst64, operands128, Operand128, {
    local: u16 = u16 / with_u16(0), value: u64 = u64 / with_u64(2)
});
operand_view!(GlobalConst32, operands64, Operand64, {
    global: u32 = u32 / with_u32(0), value: u32 = u32 / with_u32(4)
});
operand_view!(GlobalConst64, operands128, Operand128, {
    global: u32 = u32 / with_u32(0), value: u64 = u64 / with_u64(4)
});
operand_view!(LocalConstSet32, operands64, Operand64, {
    local: u16 = u16 / with_u16(0), dst: u16 = u16 / with_u16(2), value: u32 = u32 / with_u32(4)
});
operand_view!(LocalConstSet64, operands128, Operand128, {
    local: u16 = u16 / with_u16(0), dst: u16 = u16 / with_u16(2), value: u64 = u64 / with_u64(4)
});
operand_view!(MemoryFillConstOp, operands128, Operand128, {
    memory: u32 = u32 / with_u32(0), byte: u8 = u8 / with_u8(4), value: i32 = u32 / with_u32(5)
});
operand_view!(CastBranch, operands64, Operand64, {
    target: u32 = u32 / with_u32(0), ref_type_bits: u32 = u32 / with_u32(4)
});
operand_view!(BranchTableArg, operands128, Operand128, {
    target: u32 = u32 / with_u32(0), start: u32 = u32 / with_u32(4), len: u32 = u32 / with_u32(8)
});
operand_view!(StackConst32, operands64, Operand64, {
    target: u32 = u32 / with_u32(0), value: i32 = u32 / with_u32(4)
});
operand_view!(StackConst64, operands128, Operand128, {
    target: u32 = u32 / with_u32(0), value: i64 = u64 / with_u64(4)
});
operand_view!(LocalConstCmp, operands128, Operand128, {
    target: u32 = u32 / with_u32(0), value: i32 = u32 / with_u32(4), local: u16 = u16 / with_u16(8)
});
operand_view!(LocalLocalCmp, operands64, Operand64, {
    target: u32 = u32 / with_u32(0), left: u16 = u16 / with_u16(4), right: u16 = u16 / with_u16(6)
});
operand_view!(LocalUpdate, operands128, Operand128, {
    target: u32 = u32 / with_u32(0), value: i32 = u32 / with_u32(4), local: u16 = u16 / with_u16(8), on_zero: u8 = u8 / with_u8(10)
});
operand_view!(GlobalUpdate, operands128, Operand128, {
    target: u32 = u32 / with_u32(0), value: i32 = u32 / with_u32(4), global: u32 = u32 / with_u32(8), on_zero: u8 = u8 / with_u8(12)
});
operand_view!(LocalUpdateCmp, operands128, Operand128, {
    target: u32 = u32 / with_u32(0), value: i32 = u32 / with_u32(4), local: u16 = u16 / with_u16(8), right: u16 = u16 / with_u16(10)
});

/// An operation packed inline with an indexed operand.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "archive",
    serde(bound(serialize = "Op: Copy + serde::Serialize", deserialize = "Op: Copy + serde::de::DeserializeOwned",))
)]
#[repr(Rust, packed)]
pub struct PackedOp<Op, T> {
    pub op: Op,
    pub index: OperandIdx<T>,
}

impl<Op, T> PackedOp<Op, T> {
    /// Creates a packed operation from an operator and operand index.
    #[inline]
    pub const fn new(op: Op, index: OperandIdx<T>) -> Self {
        Self { op, index }
    }
}

/// A SIMD value stored in the 128-bit operand lane.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct V128Operand {
    pub value: [u8; 16],
}

/// A local SIMD operation with an indexed constant.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct LocalV128 {
    pub local: u16,
    pub value: OperandIdx<V128Operand>,
}

/// A global SIMD operation with an indexed constant.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct GlobalV128 {
    pub global: u32,
    pub value: OperandIdx<V128Operand>,
}

/// A local SIMD operation, indexed constant, and destination local.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct LocalConstSetV128 {
    pub local: u16,
    pub dst: u16,
    pub value: OperandIdx<V128Operand>,
}

macro_rules! operand_raw_view {
    ($name:ident, $raw:ty, $decode:expr, $encode:expr) => {
        impl sealed::Sealed for $name {}

        impl OperandType for $name {
            type Raw = $raw;

            #[inline(always)]
            fn decode(raw: Self::Raw) -> Self {
                $decode(raw)
            }

            #[inline]
            fn encode(self) -> Self::Raw {
                $encode(self)
            }
        }
    };
}

operand_raw_view!(V128Operand, Operand128, |raw: Operand128| Self { value: raw.to_le_bytes() }, |value: Self| {
    Operand128::from_le_bytes(value.value)
});
operand_raw_view!(
    LocalV128,
    Operand64,
    |raw: Operand64| Self { local: raw.u16::<0>(), value: OperandIdx::new(raw.u32::<2>()) },
    |value: Self| { Operand64::default().with_u16::<0>(value.local).with_u32::<2>(value.value.index()) }
);
operand_raw_view!(
    GlobalV128,
    Operand64,
    |raw: Operand64| Self { global: raw.u32::<0>(), value: OperandIdx::new(raw.u32::<4>()) },
    |value: Self| { Operand64::default().with_u32::<0>(value.global).with_u32::<4>(value.value.index()) }
);
operand_raw_view!(
    LocalConstSetV128,
    Operand64,
    |raw: Operand64| Self { local: raw.u16::<0>(), dst: raw.u16::<2>(), value: OperandIdx::new(raw.u32::<4>()) },
    |value: Self| {
        Operand64::default().with_u16::<0>(value.local).with_u16::<2>(value.dst).with_u32::<4>(value.value.index())
    }
);

impl CastBranch {
    #[inline]
    pub const fn ref_type(self) -> RefType {
        match RefType::from_bits(self.ref_type_bits) {
            Some(value) => value,
            None => unreachable!(),
        }
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

impl MemoryArg {
    #[inline]
    pub const fn new(offset: u64, mem_addr: MemAddr) -> Self {
        Self { offset, mem_addr }
    }

    #[inline]
    pub const fn offset(self) -> u64 {
        self.offset
    }

    #[inline]
    pub const fn mem_addr(self) -> MemAddr {
        self.mem_addr
    }
}

impl sealed::Sealed for MemoryArg {}

impl OperandType for MemoryArg {
    type Raw = Operand128;

    #[inline(always)]
    fn decode(raw: Self::Raw) -> Self {
        Self::new(raw.u64::<0>(), raw.u32::<8>())
    }

    #[inline]
    fn encode(self) -> Self::Raw {
        Operand128::default().with_u64::<0>(self.offset).with_u32::<8>(self.mem_addr)
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

impl sealed::Sealed for CompactMemoryArg {}

impl OperandType for CompactMemoryArg {
    type Raw = Operand64;

    #[inline(always)]
    fn decode(raw: Self::Raw) -> Self {
        Self { offset: raw.u32::<0>(), mem_addr: raw.u16::<4>() }
    }

    #[inline]
    fn encode(self) -> Self::Raw {
        Operand64::default().with_u32::<0>(self.offset).with_u16::<4>(self.mem_addr)
    }
}

impl TryFrom<MemoryArg> for CompactMemoryArg {
    type Error = ();

    #[inline]
    fn try_from(arg: MemoryArg) -> Result<Self, Self::Error> {
        Ok(Self {
            offset: u32::try_from(arg.offset()).map_err(|_| ())?,
            mem_addr: u16::try_from(arg.mem_addr()).map_err(|_| ())?,
        })
    }
}

impl From<CompactMemoryArg> for MemoryArg {
    #[inline]
    fn from(arg: CompactMemoryArg) -> Self {
        Self::new(arg.offset(), arg.mem_addr())
    }
}

const _: () = {
    assert!(core::mem::size_of::<OperandIdx<I64Operand>>() == 4);
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
    AddConst32(i32), AddConst64(OperandIdx<I64Operand>),
    IncLocal32(I32LocalArg), IncLocal64(PackedOp<LocalAddr, I64Operand>),
    // The 32/64 suffix describes the operand width. Future compare-style ops may still yield i32 results.
    BinOpLocalLocal32(BinOp, LocalAddr, LocalAddr), BinOpLocalLocal64(BinOp, LocalAddr, LocalAddr),
    BinOpLocalLocal128(BinOp128, LocalAddr, LocalAddr),
    CmpLocalLocal32(CmpOp, LocalAddr, LocalAddr), CmpLocalLocal64(CmpOp, LocalAddr, LocalAddr),
    AddLocalLocalSet32(LocalTripleArg), AddLocalLocalTee32(LocalTripleArg),
    BinOpLocalLocalSet32(PackedOp<BinOp, LocalTripleArg>), BinOpLocalLocalSet64(PackedOp<BinOp, LocalTripleArg>), BinOpLocalLocalSet128(PackedOp<BinOp128, LocalTripleArg>),
    BinOpLocalLocalTee32(PackedOp<BinOp, LocalTripleArg>), BinOpLocalLocalTee64(PackedOp<BinOp, LocalTripleArg>), BinOpLocalLocalTee128(PackedOp<BinOp128, LocalTripleArg>),
    AddLocalConst32(I32LocalArg), SubLocalConst32(I32LocalArg), MulLocalConst32(I32LocalArg),
    BinOpLocalConst32(PackedOp<BinOp, LocalU32>), BinOpLocalConst64(PackedOp<BinOp, LocalConst64>),
    BinOpGlobalConst32(PackedOp<BinOp, GlobalConst32>), BinOpGlobalConst64(PackedOp<BinOp, GlobalConst64>),
    BinOpGlobalConst128(PackedOp<BinOp128, GlobalV128>), BinOpLocalConst128(PackedOp<BinOp128, LocalV128>),
    BinOpLocalConstSet32(PackedOp<BinOp, LocalConstSet32>), BinOpLocalConstSet64(PackedOp<BinOp, LocalConstSet64>), BinOpLocalConstSet128(PackedOp<BinOp128, LocalConstSetV128>),
    BinOpLocalConstTee32(PackedOp<BinOp, LocalConstSet32>), BinOpLocalConstTee64(PackedOp<BinOp, LocalConstSet64>), BinOpLocalConstTee128(PackedOp<BinOp128, LocalConstSetV128>),
    BinOpStackLocal32(BinOp, LocalAddr),
    BinOpStackLocalSet32(BinOp, LocalAddr, LocalAddr),
    BinOpStackLocalTee32(BinOp, LocalAddr, LocalAddr),
    BinOpStackLocal128(BinOp128, LocalAddr),
    BinOpStackGlobal32(BinOp, u32),
    BinOpStackGlobal64(BinOp, u32),
    SetLocalConst32(I32LocalArg), SetLocalConst64(PackedOp<LocalAddr, I64Operand>), SetLocalConst128(PackedOp<LocalAddr, V128Operand>),
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
    AndConstTee64(PackedOp<LocalAddr, I64Operand>), SubConstTee64(PackedOp<LocalAddr, I64Operand>),
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
    JumpCmpStackConst32(PackedOp<CmpOp, StackConst32>), JumpCmpStackConst64(PackedOp<CmpOp, StackConst64>),
    JumpCmpStackLocal32(PackedOp<CmpOp, TargetLocal>), JumpCmpStackLocal64(PackedOp<CmpOp, TargetLocal>),
    BinOpLocalConstJump32(PackedOp<BinOp, LocalUpdate>), BinOpLocalConstJumpCmpLocal32(PackedOp<(BinOp, CmpOp), LocalUpdateCmp>),
    BinOpStackConstTeeLocalJump32(PackedOp<BinOp, LocalUpdate>), BinOpGlobalConstJump32(PackedOp<BinOp, GlobalUpdate>),
    IncLocalJump32(OperandIdx<LocalUpdate>), IncStackTeeLocalJump32(OperandIdx<LocalUpdate>), IncGlobalJump32(OperandIdx<GlobalUpdate>),
    IncLocalJumpCmpLocal32(PackedOp<CmpOp, LocalUpdateCmp>), JumpCmpLocalConst32(PackedOp<CmpOp, LocalConstCmp>), JumpCmpLocalConst64(PackedOp<CmpOp, LocalConstCmp>),
    JumpCmpLocalLocal32(PackedOp<CmpOp, LocalLocalCmp>), JumpCmpLocalLocal64(PackedOp<CmpOp, LocalLocalCmp>),
    DropKeep32 { base: u16, keep: u16 }, DropKeep64 { base: u16, keep: u16 }, DropKeep128 { base: u16, keep: u16 }, BranchTable(OperandIdx<BranchTableArg>),
    Return,
    ReturnVoid,
    Return32,
    Return64,
    Return128,
    Call(FuncAddr),
    CallSelf,
    CallIndirect(OperandIdx<TwoU32>),
    CallRef(TypeAddr),
    ReturnCall(FuncAddr),
    ReturnCallSelf,
    ReturnCallIndirect(OperandIdx<TwoU32>),
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
    I32Load(OperandIdx<MemoryArg>), I64Load(OperandIdx<MemoryArg>), F32Load(OperandIdx<MemoryArg>), F64Load(OperandIdx<MemoryArg>),
    I32Load8S(OperandIdx<MemoryArg>), I32Load8U(OperandIdx<MemoryArg>), I32Load16S(OperandIdx<MemoryArg>), I32Load16U(OperandIdx<MemoryArg>),
    I64Load8S(OperandIdx<MemoryArg>), I64Load8U(OperandIdx<MemoryArg>), I64Load16S(OperandIdx<MemoryArg>), I64Load16U(OperandIdx<MemoryArg>), I64Load32S(OperandIdx<MemoryArg>), I64Load32U(OperandIdx<MemoryArg>),
    I32Store(OperandIdx<MemoryArg>), I64Store(OperandIdx<MemoryArg>), F32Store(OperandIdx<MemoryArg>), F64Store(OperandIdx<MemoryArg>),
    I32Store8(OperandIdx<MemoryArg>), I32Store16(OperandIdx<MemoryArg>), I64Store8(OperandIdx<MemoryArg>), I64Store16(OperandIdx<MemoryArg>), I64Store32(OperandIdx<MemoryArg>),
    MemorySize(MemAddr),
    MemoryGrow(MemAddr),

    // > Constants
    Const32(i32),
    Const64Imm(i32),
    Const64(OperandIdx<I64Operand>),

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
    BrOnCast(OperandIdx<CastBranch>),
    BrOnCastFail(OperandIdx<CastBranch>),

    // > GC Objects
    StructNew(TypeAddr),
    StructNewDefault(TypeAddr),
    StructGet(OperandIdx<TwoU32>), StructGetS(OperandIdx<TwoU32>), StructGetU(OperandIdx<TwoU32>), StructSet(OperandIdx<TwoU32>),
    ArrayNew(TypeAddr),
    ArrayNewDefault(TypeAddr),
    ArrayNewFixed(OperandIdx<TwoU32>),
    ArrayNewData(OperandIdx<TwoU32>), ArrayNewElem(OperandIdx<TwoU32>),
    ArrayGet(TypeAddr),
    ArrayGetS(TypeAddr),
    ArrayGetU(TypeAddr),
    ArraySet(TypeAddr),
    ArrayLen,
    ArrayFill(TypeAddr),
    ArrayCopy(OperandIdx<TwoU32>), ArrayInitData(OperandIdx<TwoU32>), ArrayInitElem(OperandIdx<TwoU32>),

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
    TableInit(OperandIdx<TwoU32>),
    TableGet(TableAddr),
    TableSet(TableAddr),
    TableCopy(OperandIdx<TwoU32>),
    TableGrow(TableAddr),
    TableSize(TableAddr),
    TableFill(TableAddr),

    // > Bulk Memory Instructions
    MemoryInit(OperandIdx<TwoU32>), MemoryCopy(OperandIdx<TwoU32>),
    MemoryFill(MemAddr),
    MemoryFillConst(OperandIdx<MemoryFillConstOp>),
    DataDrop(DataAddr),
    ElemDrop(ElemAddr),

    // > Wide Arithmetic
    I64Add128, I64Sub128, I64MulWideS, I64MulWideU,

    // > SIMD
    V128Load(OperandIdx<MemoryArg>), V128Load8x8S(OperandIdx<MemoryArg>), V128Load8x8U(OperandIdx<MemoryArg>),
    V128Load16x4S(OperandIdx<MemoryArg>), V128Load16x4U(OperandIdx<MemoryArg>), V128Load32x2S(OperandIdx<MemoryArg>), V128Load32x2U(OperandIdx<MemoryArg>),

    V128Load8Splat(OperandIdx<MemoryArg>), V128Load16Splat(OperandIdx<MemoryArg>), V128Load32Splat(OperandIdx<MemoryArg>), V128Load64Splat(OperandIdx<MemoryArg>),
    V128Load8Lane(MemoryLaneArg), V128Load16Lane(MemoryLaneArg), V128Load32Lane(MemoryLaneArg), V128Load64Lane(MemoryLaneArg),

    V128Load32Zero(OperandIdx<MemoryArg>), V128Load64Zero(OperandIdx<MemoryArg>),

    V128Store(OperandIdx<MemoryArg>), V128Store8Lane(MemoryLaneArg), V128Store16Lane(MemoryLaneArg), V128Store32Lane(MemoryLaneArg), V128Store64Lane(MemoryLaneArg),

    I8x16Shuffle(OperandIdx<V128Operand>),
    Const128Imm(u32), Const128(OperandIdx<V128Operand>),

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
}

const _: () = assert!(core::mem::size_of::<Instruction>() == 8);

#[cfg(test)]
mod tests {
    use alloc::vec;

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
        let memory = MemoryArg::new(u64::MAX, u32::MAX);
        assert_eq!(MemoryArg::decode(memory.encode()), memory);

        let value = Operand128::default()
            .with_u8::<0>(u8::MAX)
            .with_u16::<1>(u16::MAX)
            .with_u32::<3>(u32::MAX)
            .with_u64::<7>(u64::MAX);
        assert_eq!(
            (value.u8::<0>(), value.u16::<1>(), value.u32::<3>(), value.u64::<7>()),
            (u8::MAX, u16::MAX, u32::MAX, u64::MAX)
        );

        let bytes = [u8::MAX; 16];
        assert_eq!(Operand128::from_le_bytes(bytes).to_le_bytes(), bytes);
    }

    #[test]
    fn v128_operand_views_round_trip_max_bytes() {
        let value = OperandIdx::<V128Operand>::new(0);
        let local = OperandIdx::<LocalV128>::new(0);
        let global = OperandIdx::<GlobalV128>::new(1);
        let set = OperandIdx::<LocalConstSetV128>::new(2);
        let data = super::super::WasmFunctionData {
            operands128: vec![V128Operand { value: [u8::MAX; 16] }.encode()].into_boxed_slice(),
            operands64: vec![
                LocalV128 { local: u16::MAX, value }.encode(),
                GlobalV128 { global: u32::MAX, value }.encode(),
                LocalConstSetV128 { local: u16::MAX, dst: u16::MAX, value }.encode(),
            ]
            .into_boxed_slice(),
            ..Default::default()
        };

        assert_eq!(value.get(&data).value, [u8::MAX; 16]);
        assert_eq!(local.get(&data), LocalV128 { local: u16::MAX, value });
        assert_eq!(global.get(&data), GlobalV128 { global: u32::MAX, value });
        assert_eq!(set.get(&data), LocalConstSetV128 { local: u16::MAX, dst: u16::MAX, value });
    }
}
