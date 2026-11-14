//! Inline instruction arguments and typed operand-pool layouts.

use core::marker::PhantomData;

use crate::{LocalAddr, operations::BinOp};

macro_rules! read_field {
    ($self:ident, bool, $offset:expr) => {{ $self.bytes[$offset] != 0 }};
    ($self:ident, u8, $offset:expr) => {{ $self.bytes[$offset] }};
    ($self:ident, $kind:ident, $offset:expr) => {{
        const LEN: usize = core::mem::size_of::<$kind>();
        match <[u8; LEN]>::try_from(&$self.bytes[$offset..$offset + LEN]) {
            Ok(bytes) => $kind::from_le_bytes(bytes),
            Err(_) => {
                core::hint::cold_path();
                unreachable!("invalid operand field read")
            }
        }
    }};
}

macro_rules! write_field {
    ($self:ident, bool, $offset:expr, $value:expr) => {{ write_field!($self, u8, $offset, u8::from($value)) }};
    ($self:ident, $kind:ident, $offset:expr, $value:expr) => {{
        let value: $kind = $value;
        $self.write::<{ $offset }, _>(value.to_le_bytes())
    }};
}

macro_rules! define_operand {
    ($name:ident, $len:literal) => {
        #[doc = concat!("A typed packed ", $len, "-byte operand value.")]
        #[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "archive", serde(transparent, bound = ""))]
        #[repr(C)]
        pub struct $name<T = ()> {
            bytes: [u8; $len],
            #[cfg_attr(feature = "archive", serde(skip))]
            marker: PhantomData<T>,
        }

        impl<T> $name<T> {
            #[inline]
            fn write<const OFFSET: usize, const N: usize>(&mut self, bytes: [u8; N]) {
                const { assert!(OFFSET + N <= $len) };
                self.bytes[OFFSET..OFFSET + N].copy_from_slice(&bytes);
            }

            /// The jump target at the start of a retargetable operand.
            #[inline(always)]
            pub fn target(&self) -> u32 {
                read_field!(self, u32, 0)
            }

            /// Replaces the jump target stored at the start of a retargetable operand.
            #[inline]
            pub fn with_target(mut self, target: u32) -> Self {
                write_field!(self, u32, 0, target);
                self
            }

            /// Changes the layout tag without modifying the packed bytes.
            #[inline(always)]
            #[doc(hidden)]
            pub fn cast<U>(self) -> $name<U> {
                $name { bytes: self.bytes, marker: PhantomData }
            }
        }

        impl<T> Copy for $name<T> {}
        impl<T> Clone for $name<T> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<T> PartialEq for $name<T> {
            fn eq(&self, other: &Self) -> bool {
                self.bytes == other.bytes
            }
        }
        impl<T> Eq for $name<T> {}
        impl<T> PartialOrd for $name<T> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl<T> Ord for $name<T> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.bytes.cmp(&other.bytes)
            }
        }
        impl<T> Default for $name<T> {
            fn default() -> Self {
                Self { bytes: [0; $len], marker: PhantomData }
            }
        }

        #[cfg(feature = "debug")]
        impl<T> core::fmt::Debug for $name<T> {
            fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                self.bytes.fmt(formatter)
            }
        }
    };
}

macro_rules! packed_layout {
    ($(#[$meta:meta])* $operand:ident, $layout:ident { $($fields:tt)* }) => {
        $(#[$meta])*
        pub enum $layout {}
        packed_layout!(@impl $operand, $layout; $($fields)*);
    };
    ($operand:ident { $($field:ident: $kind:ident @ $offset:tt),+ $(,)? }) => {
        packed_layout!(@impl $operand, packed_layout!(@type $($kind),+); $($field: $kind @ $offset),+);
    };
    (@type $kind:ident) => { $kind };
    (@type $($kind:ident),+) => { ($($kind),+) };
    (@impl $operand:ident, $layout:ty; $($field:ident: $kind:ident @ $offset:tt),+ $(,)?) => {
        impl $operand<$layout> {
            /// Packs the fields into a pool operand.
            #[inline]
            pub fn new($($field: $kind),+) -> Self {
                let mut operand = Self::default();
                $(write_field!(operand, $kind, $offset, $field);)+
                operand
            }

            $(packed_layout!(@getter $field: $kind @ $offset);)+
        }
    };
    // The common accessor already reads the first u32 of a branch operand.
    (@getter target: u32 @ 0) => {};
    (@getter $field:ident: $kind:ident @ $offset:literal) => {
        #[doc = concat!("The `", stringify!($field), "` field.")]
        #[inline(always)]
        pub fn $field(&self) -> $kind {
            read_field!(self, $kind, $offset)
        }
    };
}

define_operand!(Operand64, 8);
define_operand!(Operand128, 16);

/// An index into the operand lane used by `T`.
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "archive", serde(transparent))]
#[repr(transparent)]
pub struct OperandIdx<T, const WIDTH: usize> {
    index: u32,
    #[cfg_attr(feature = "archive", serde(skip))]
    marker: PhantomData<T>,
}

/// An index into the 64-bit operand lane.
pub type Operand64Idx<T> = OperandIdx<T, 8>;

/// An index into the 128-bit operand lane.
pub type Operand128Idx<T> = OperandIdx<T, 16>;

impl<T, const WIDTH: usize> OperandIdx<T, WIDTH> {
    /// Creates a typed index into an operand pool.
    #[inline]
    #[doc(hidden)]
    pub const fn new(index: u32) -> Self {
        Self { index, marker: PhantomData }
    }

    /// The index within the operand pool.
    #[inline]
    pub const fn index(self) -> usize {
        self.index as usize
    }
}

impl<T, const WIDTH: usize> Copy for OperandIdx<T, WIDTH> {}
impl<T, const WIDTH: usize> Clone for OperandIdx<T, WIDTH> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, const WIDTH: usize> PartialEq for OperandIdx<T, WIDTH> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
impl<T, const WIDTH: usize> Eq for OperandIdx<T, WIDTH> {}

#[cfg(feature = "debug")]
impl<T, const WIDTH: usize> core::fmt::Debug for OperandIdx<T, WIDTH> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.index.fmt(formatter)
    }
}

impl<T> Operand64Idx<T> {
    /// Resolves this index to a copied 64-bit packed operand.
    #[inline(always)]
    pub fn resolve(self, data: &crate::WasmFunctionData) -> Operand64<T> {
        data.operands64[self.index as usize].cast()
    }
}

impl<T> Operand128Idx<T> {
    /// Resolves this index to a copied 128-bit packed operand.
    #[inline(always)]
    pub fn resolve(self, data: &crate::WasmFunctionData) -> Operand128<T> {
        data.operands128[self.index as usize].cast()
    }
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

/// A wasm32 memory-zero load with an inline local and small offset.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(C, packed)]
pub struct MemoryLocalCompactArg {
    /// Frame-local address operand in the 32-bit lane.
    pub local: u16,
    /// Non-wrapping byte offset applied to the wasm32 base address.
    pub offset: u16,
}

/// An indexed scalar memory operand with its statically known address width.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(Rust, packed)]
pub struct MemoryArg {
    pub memory_arg_idx: Operand128Idx<MemoryOperand>,
    pub address64: bool,
}

/// An inline generic 32-bit operation with a small local index and full-width immediate.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(C, packed)]
pub struct BinOpLocalCompact32 {
    pub op: BinOp,
    pub local: u8,
    pub value: u32,
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

packed_layout!(Operand64 { value: i64 @ 0 });
packed_layout!(Operand64 { a: u32 @ 0, b: u32 @ 4 });
packed_layout!(Operand64 { a: u16 @ 0, b: u32 @ 2 });
packed_layout!(Operand64 { a: u32 @ 0, b: u16 @ 4 });
packed_layout!(Operand64 { a: u16 @ 0, b: u16 @ 2, c: u16 @ 4 });
packed_layout!(Operand64 { a: u16 @ 0, b: u16 @ 2, c: u32 @ 4 });
packed_layout!(Operand64 { a: u8 @ 0, b: u16 @ 1, c: u32 @ 3 });
packed_layout!(Operand64 { a: u32 @ 0, b: i32 @ 4 });
packed_layout!(Operand64 { a: u32 @ 0, b: u16 @ 4, c: u16 @ 6 });
packed_layout!(Operand128 { a: u16 @ 0, b: u64 @ 2 });
packed_layout!(Operand128 { a: u32 @ 0, b: u64 @ 4 });
packed_layout!(Operand128 { a: u16 @ 0, b: u16 @ 2, c: u64 @ 4 });
packed_layout!(Operand128 { a: u32 @ 0, b: i64 @ 4 });
packed_layout!(Operand128 { a: u32 @ 0, b: i32 @ 4, c: u16 @ 8 });
packed_layout!(Operand128 { a: u32 @ 0, b: i64 @ 4, c: u16 @ 12 });

packed_layout! {
    /// Identifies the packed full-width memory operand layout.
    Operand128, MemoryOperand { offset: u64 @ 0, memory: u32 @ 8 }
}

packed_layout! {
    /// A compact pooled memory operand.
    Operand64, CompactMemoryOperand { offset: u32 @ 0, memory: u16 @ 4, address64: bool @ 6 }
}

packed_layout! {
    /// Identifies a packed branch-table operand.
    Operand128, BranchTableOperand { target: u32 @ 0, start: u32 @ 4, size: u32 @ 8 }
}

packed_layout! {
    /// Identifies a packed constant memory-fill operand.
    Operand128, MemoryFillOperand { memory: u32 @ 0, byte: u8 @ 4, value: i32 @ 5 }
}

packed_layout! {
    /// Identifies a packed local update and branch operand.
    Operand128, LocalUpdateOperand { target: u32 @ 0, value: i32 @ 4, local: u16 @ 8, on_zero: bool @ 10 }
}

packed_layout! {
    /// Identifies a packed global update and branch operand.
    Operand128, GlobalUpdateOperand { target: u32 @ 0, value: i32 @ 4, global: u32 @ 8, on_zero: bool @ 12 }
}

packed_layout! {
    /// Identifies a packed local update, comparison, and branch operand.
    Operand128, LocalUpdateCmpOperand { target: u32 @ 0, value: i32 @ 4, local: u16 @ 8, right: u16 @ 10 }
}

packed_layout! {
    /// Identifies a packed 64-bit local/constant update, comparison, and branch operand.
    Operand128, LocalUpdateCmpOperand64 { target: u32 @ 0, value: i64 @ 4, local: u16 @ 12, right: u16 @ 14 }
}

packed_layout! {
    /// Identifies a local/local operation written to a local and compared with another local.
    Operand128, LocalBinOpCmpOperand { target: u32 @ 0, left: u16 @ 4, right: u16 @ 6, destination: u16 @ 8, compared: u16 @ 10 }
}

packed_layout! {
    /// Identifies a local/constant expression compared with a constant branch operand.
    Operand128, LocalExprCmpOperand { target: u32 @ 0, local: u16 @ 4, value: u32 @ 6, right: i32 @ 10 }
}

/// Identifies a packed local-address load, local tee, and branch operand.
pub enum MemoryLocalJumpOperand {}

/// Identifies a local-address memory update with source and saved-value locals.
pub enum MemoryUpdateOperand {}

impl Operand128<[u8; 16]> {
    /// Packs the full byte array as an operand.
    #[inline]
    pub fn new(value: [u8; 16]) -> Self {
        Self { bytes: value, marker: PhantomData }
    }

    /// The stored byte array.
    #[inline(always)]
    pub fn value(&self) -> [u8; 16] {
        self.bytes
    }
}

impl Operand64<CompactMemoryOperand> {
    /// Packs a full-width memory operand if its offset and index fit.
    #[inline]
    pub fn try_from_memory(memory: Operand128<MemoryOperand>, address64: bool) -> Option<Self> {
        Some(Self::new(u32::try_from(memory.offset()).ok()?, u16::try_from(memory.memory()).ok()?, address64))
    }
}

impl Operand128<MemoryUpdateOperand> {
    /// Creates an update operand, `old_local` is used only by replacement updates.
    #[inline]
    pub fn new(memory: Operand64<CompactMemoryOperand>, address: u8, value: LocalAddr, old_local: LocalAddr) -> Self {
        let mut operand = Self::default();
        operand.bytes[..8].copy_from_slice(&memory.bytes);
        write_field!(operand, u8, 8, address);
        write_field!(operand, u16, 9, value);
        write_field!(operand, u16, 11, old_local);
        operand
    }

    /// The inline compact memory operand.
    #[inline(always)]
    pub fn memory(&self) -> Operand64<CompactMemoryOperand> {
        Operand64::<i64>::new(read_field!(self, i64, 0)).cast()
    }

    /// The local holding the address.
    #[inline(always)]
    pub fn address(&self) -> u8 {
        read_field!(self, u8, 8)
    }

    /// The local supplying the replacement or right-hand operand.
    #[inline(always)]
    pub fn value_local(&self) -> LocalAddr {
        read_field!(self, u16, 9)
    }

    /// The local that receives the old word during replacement.
    #[inline(always)]
    pub fn old_local(&self) -> LocalAddr {
        read_field!(self, u16, 11)
    }
}

impl Operand128<MemoryLocalJumpOperand> {
    /// Packs a branch target and the existing local-address load operands.
    #[inline]
    pub fn new(target: u32, arg: MemoryLocalArg) -> Self {
        let mut operand = Self::default();
        write_field!(operand, u32, 0, target);
        write_field!(operand, u32, 4, arg.memory_arg_idx.index() as u32);
        write_field!(operand, u8, 8, arg.local1);
        write_field!(operand, u8, 9, arg.local2);
        operand
    }

    /// The local-address load and tee operands.
    #[inline(always)]
    pub fn memory(&self) -> MemoryLocalArg {
        MemoryLocalArg {
            memory_arg_idx: Operand64Idx::new(read_field!(self, u32, 4)),
            local1: read_field!(self, u8, 8),
            local2: read_field!(self, u8, 9),
        }
    }
}

const _: () = {
    assert!(core::mem::size_of::<Operand64Idx<i64>>() == 4);
    assert!(core::mem::size_of::<Operand64>() == 8);
    assert!(core::mem::size_of::<Operand128>() == 16);
    assert!(core::mem::size_of::<MemoryLocalArg>() == 6);
    assert!(core::mem::size_of::<BinOpLocalCompact32>() == 6);
};
