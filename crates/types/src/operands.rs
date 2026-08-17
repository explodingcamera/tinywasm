use core::marker::PhantomData;

use crate::WasmFunctionData;

/// An index into the operand lane used by `T`.
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "archive", serde(transparent))]
#[repr(transparent)]
pub struct OperandIdx<T> {
    index: u32,
    #[cfg_attr(feature = "archive", serde(skip))]
    marker: PhantomData<T>,
}

impl<T> OperandIdx<T> {
    #[inline]
    #[doc(hidden)]
    pub const fn new(index: u32) -> Self {
        Self { index, marker: PhantomData }
    }

    /// Returns the underlying lane index.
    #[inline]
    pub const fn index(self) -> u32 {
        self.index
    }
}

impl<T: OperandType> OperandIdx<T> {
    /// Returns the typed operand at this index.
    ///
    /// Panics if the index is outside its statically selected operand lane.
    #[inline(always)]
    pub fn get(self, data: &WasmFunctionData) -> T {
        T::decode(T::Raw::get(data, self.index))
    }
}

impl<T> Copy for OperandIdx<T> {}

impl<T> Clone for OperandIdx<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for OperandIdx<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl<T> Eq for OperandIdx<T> {}

#[cfg(feature = "debug")]
impl<T> core::fmt::Debug for OperandIdx<T> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.index.fmt(formatter)
    }
}

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// A typed view stored in one of the per-function operand lanes.
pub trait OperandType: sealed::Sealed + Copy {
    #[doc(hidden)]
    type Raw: RawOperand;

    #[doc(hidden)]
    fn decode(raw: Self::Raw) -> Self;
    #[doc(hidden)]
    fn encode(self) -> Self::Raw;
}

#[doc(hidden)]
pub trait RawOperand: sealed::Sealed + Copy {
    fn get(data: &WasmFunctionData, index: u32) -> Self;
}

macro_rules! define_operand {
    ($name:ident, $len:literal, $lane:ident) => {
        #[doc = concat!("Raw fields stored in a per-function ", $len, "-byte operand lane.")]
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
        #[cfg_attr(feature = "debug", derive(Debug))]
        #[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
        #[repr(transparent)]
        pub struct $name([u8; $len]);

        impl sealed::Sealed for $name {}

        impl RawOperand for $name {
            #[inline(always)]
            fn get(data: &WasmFunctionData, index: u32) -> Self {
                *data.$lane.get(index as usize).unwrap_or_else(|| unreachable!("invalid operand index"))
            }
        }

        impl $name {
            /// Returns the byte at `OFFSET`.
            #[inline]
            pub fn u8<const OFFSET: usize>(self) -> u8 {
                const { assert!(OFFSET < $len) };
                self.0[OFFSET]
            }

            /// Returns the little-endian `u16` at `OFFSET`.
            #[inline]
            pub fn u16<const OFFSET: usize>(self) -> u16 {
                const { assert!(OFFSET + 2 <= $len) };
                u16::from_le_bytes(self.0[OFFSET..OFFSET + 2].try_into().unwrap_or_else(|_| unreachable!()))
            }

            /// Returns the little-endian `u32` at `OFFSET`.
            #[inline]
            pub fn u32<const OFFSET: usize>(self) -> u32 {
                const { assert!(OFFSET + 4 <= $len) };
                u32::from_le_bytes(self.0[OFFSET..OFFSET + 4].try_into().unwrap_or_else(|_| unreachable!()))
            }

            /// Returns the little-endian `u64` at `OFFSET`.
            #[inline]
            pub fn u64<const OFFSET: usize>(self) -> u64 {
                const { assert!(OFFSET + 8 <= $len) };
                u64::from_le_bytes(self.0[OFFSET..OFFSET + 8].try_into().unwrap_or_else(|_| unreachable!()))
            }

            /// Builds an operand from its raw little-endian bytes.
            #[inline]
            pub fn from_le_bytes(value: [u8; $len]) -> Self {
                Self(value)
            }

            /// Returns the raw little-endian bytes.
            #[inline]
            pub fn to_le_bytes(self) -> [u8; $len] {
                self.0
            }

            /// Writes `value` at `OFFSET` and returns the updated operand.
            #[inline]
            pub fn with_u8<const OFFSET: usize>(mut self, value: u8) -> Self {
                const { assert!(OFFSET < $len) };
                self.0[OFFSET] = value;
                self
            }

            /// Writes `value` at `OFFSET` and returns the updated operand.
            #[inline]
            pub fn with_u16<const OFFSET: usize>(mut self, value: u16) -> Self {
                const { assert!(OFFSET + 2 <= $len) };
                self.0[OFFSET..OFFSET + 2].copy_from_slice(&value.to_le_bytes());
                self
            }

            /// Writes `value` at `OFFSET` and returns the updated operand.
            #[inline]
            pub fn with_u32<const OFFSET: usize>(mut self, value: u32) -> Self {
                const { assert!(OFFSET + 4 <= $len) };
                self.0[OFFSET..OFFSET + 4].copy_from_slice(&value.to_le_bytes());
                self
            }

            /// Writes `value` at `OFFSET` and returns the updated operand.
            #[inline]
            pub fn with_u64<const OFFSET: usize>(mut self, value: u64) -> Self {
                const { assert!(OFFSET + 8 <= $len) };
                self.0[OFFSET..OFFSET + 8].copy_from_slice(&value.to_le_bytes());
                self
            }
        }
    };
}

define_operand!(Operand64, 8, operands64);
define_operand!(Operand128, 16, operands128);
