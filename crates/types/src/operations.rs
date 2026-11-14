//! Operation selectors carried by TinyWasm bytecode instructions.

use crate::{MemoryOperand, Operand128Idx};

/// An integer comparison operator used for local comparisons and conditional jumps.
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

/// A floating-point comparison, with unordered behavior matching WebAssembly.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum FloatCmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
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

/// Operation performed by an atomic memory instruction.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum AtomicOp {
    Load = 0,
    Store = 1,
    Add = 2,
    Sub = 3,
    And = 4,
    Or = 5,
    Xor = 6,
    Xchg = 7,
    Cmpxchg = 8,
}

/// Width of an atomic memory access, in bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum AtomicWidth {
    Bytes1 = 1,
    Bytes2 = 2,
    Bytes4 = 4,
    Bytes8 = 8,
}

impl AtomicWidth {
    /// Converts a supported byte width into an atomic width.
    pub const fn from_bytes(bytes: u8) -> Self {
        match bytes {
            1 => Self::Bytes1,
            2 => Self::Bytes2,
            4 => Self::Bytes4,
            8 => Self::Bytes8,
            _ => panic!("invalid atomic width"),
        }
    }
}

/// An atomic operation with its module-local memory operand and access width.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(C, packed)]
pub struct AtomicArg {
    pub memory: Operand128Idx<MemoryOperand>,
    code: u8,
}

/// Operation performed by an atomic wait or notify instruction.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum AtomicWaitOp {
    Notify,
    Wait32,
    Wait64,
}

impl AtomicArg {
    /// Packs the operation, access width, and value type into one byte.
    pub const fn new(memory: Operand128Idx<MemoryOperand>, width: AtomicWidth, is_64: bool, op: AtomicOp) -> Self {
        Self { memory, code: (op as u8) | ((width as u8).trailing_zeros() as u8) << 4 | (is_64 as u8) << 6 }
    }

    /// Returns the access width.
    pub const fn width(self) -> AtomicWidth {
        match (self.code >> 4) & 3 {
            0 => AtomicWidth::Bytes1,
            1 => AtomicWidth::Bytes2,
            2 => AtomicWidth::Bytes4,
            _ => AtomicWidth::Bytes8,
        }
    }
    /// Returns whether the value is an i64.
    pub const fn is_64(self) -> bool {
        self.code & 64 != 0
    }
    /// Returns the atomic operation.
    pub fn op(self) -> AtomicOp {
        match self.code & 15 {
            0 => AtomicOp::Load,
            1 => AtomicOp::Store,
            2 => AtomicOp::Add,
            3 => AtomicOp::Sub,
            4 => AtomicOp::And,
            5 => AtomicOp::Or,
            6 => AtomicOp::Xor,
            7 => AtomicOp::Xchg,
            8 => AtomicOp::Cmpxchg,
            _ => unreachable!("invalid atomic operation"),
        }
    }
}

/// Scalar load and extension used by a generic 32-bit memory/arithmetic fusion.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum LoadKind32 {
    U32,
    I8,
    U8,
    I16,
    U16,
}

/// Operation applied to a word loaded from a local-addressed memory location.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum MemoryUpdateOp {
    /// Saves the old word in a local and replaces it from another local.
    Replace,
    /// Combines the old word with a local operand and stores the result.
    BinOp(BinOp),
}
