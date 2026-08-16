use core::marker::PhantomData;

use crate::WasmFunctionData;

/// An index into the operand lane used by `T`.
#[repr(transparent)]
pub struct OperandIdx<T> {
    index: u32,
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

#[cfg(feature = "archive")]
impl<T> serde::Serialize for OperandIdx<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.index, serializer)
    }
}

#[cfg(feature = "archive")]
impl<'de, T> serde::Deserialize<'de> for OperandIdx<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::new(serde::Deserialize::deserialize(deserializer)?))
    }
}

/// Raw fields stored in a per-function 64-bit operand lane.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
pub struct Operand64(u64);

/// Raw fields stored in a per-function 128-bit operand lane.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
pub struct Operand128(u128);

pub(crate) mod sealed {
    pub trait Sealed {}
    pub trait RawSealed {}
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
pub trait RawOperand: sealed::RawSealed + Copy {
    fn get(data: &WasmFunctionData, index: u32) -> Self;
}

impl sealed::RawSealed for Operand64 {}
impl RawOperand for Operand64 {
    #[inline(always)]
    fn get(data: &WasmFunctionData, index: u32) -> Self {
        *data.operands64.get(index as usize).unwrap_or_else(|| unreachable!("invalid operand index"))
    }
}

impl sealed::RawSealed for Operand128 {}
impl RawOperand for Operand128 {
    #[inline(always)]
    fn get(data: &WasmFunctionData, index: u32) -> Self {
        *data.operands128.get(index as usize).unwrap_or_else(|| unreachable!("invalid operand index"))
    }
}

macro_rules! operand_fields {
    ($ty:ty) => {
        #[inline]
        pub(crate) const fn u16(self, offset: u32) -> u16 {
            (self.0 >> (offset * 8)) as u16
        }
        #[inline]
        pub(crate) const fn u32(self, offset: u32) -> u32 {
            (self.0 >> (offset * 8)) as u32
        }
        #[inline]
        pub(crate) const fn u64(self, offset: u32) -> u64 {
            (self.0 >> (offset * 8)) as u64
        }
        #[inline]
        pub(crate) const fn i64(self, offset: u32) -> i64 {
            self.u64(offset) as i64
        }
    };
}

impl Operand64 {
    operand_fields!(u64);

    #[inline]
    pub(crate) const fn with_u16(self, offset: u32, value: u16) -> Self {
        let shift = offset * 8;
        Self((self.0 & !((u16::MAX as u64) << shift)) | ((value as u64) << shift))
    }

    #[inline]
    pub(crate) const fn with_u32(self, offset: u32, value: u32) -> Self {
        let shift = offset * 8;
        Self((self.0 & !((u32::MAX as u64) << shift)) | ((value as u64) << shift))
    }

    #[inline]
    pub(crate) const fn with_u64(self, _offset: u32, value: u64) -> Self {
        Self(value)
    }
}

impl Operand128 {
    operand_fields!(u128);

    #[inline]
    pub(crate) const fn u8(self, offset: u32) -> u8 {
        (self.0 >> (offset * 8)) as u8
    }

    #[inline]
    pub(crate) const fn i32(self, offset: u32) -> i32 {
        self.u32(offset) as i32
    }

    #[inline]
    pub(crate) const fn from_le_bytes(value: [u8; 16]) -> Self {
        Self(u128::from_le_bytes(value))
    }

    #[inline]
    pub(crate) const fn to_le_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    #[inline]
    pub(crate) const fn with_u8(self, offset: u32, value: u8) -> Self {
        let shift = offset * 8;
        Self((self.0 & !((u8::MAX as u128) << shift)) | ((value as u128) << shift))
    }

    #[inline]
    pub(crate) const fn with_u16(self, offset: u32, value: u16) -> Self {
        let shift = offset * 8;
        Self((self.0 & !((u16::MAX as u128) << shift)) | ((value as u128) << shift))
    }

    #[inline]
    pub(crate) const fn with_u32(self, offset: u32, value: u32) -> Self {
        let shift = offset * 8;
        Self((self.0 & !((u32::MAX as u128) << shift)) | ((value as u128) << shift))
    }

    #[inline]
    pub(crate) const fn with_u64(self, offset: u32, value: u64) -> Self {
        let shift = offset * 8;
        Self((self.0 & !((u64::MAX as u128) << shift)) | ((value as u128) << shift))
    }
}
