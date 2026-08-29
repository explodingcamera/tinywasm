use core::marker::PhantomData;

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
    #[inline]
    #[doc(hidden)]
    pub const fn new(index: u32) -> Self {
        Self { index, marker: PhantomData }
    }

    /// Returns the underlying lane index.
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

macro_rules! define_operand {
    ($name:ident, $len:literal) => {
        #[doc = concat!("A typed packed ", $len, "-byte operand value.")]
        #[repr(C)]
        pub struct $name<T = ()> {
            pub(crate) bytes: [u8; $len],
            marker: PhantomData<T>,
        }

        #[allow(dead_code)]
        impl<T> $name<T> {
            #[inline]
            pub(crate) fn write_u8<const OFFSET: usize>(&mut self, value: u8) {
                const { assert!(OFFSET < $len) };
                self.bytes[OFFSET] = value;
            }

            #[inline]
            pub(crate) fn write_u16<const OFFSET: usize>(&mut self, value: u16) {
                const { assert!(OFFSET + 2 <= $len) };
                self.bytes[OFFSET..OFFSET + 2].copy_from_slice(&value.to_le_bytes());
            }

            #[inline]
            pub(crate) fn write_u32<const OFFSET: usize>(&mut self, value: u32) {
                const { assert!(OFFSET + 4 <= $len) };
                self.bytes[OFFSET..OFFSET + 4].copy_from_slice(&value.to_le_bytes());
            }

            #[inline]
            pub(crate) fn write_u64<const OFFSET: usize>(&mut self, value: u64) {
                const { assert!(OFFSET + 8 <= $len) };
                self.bytes[OFFSET..OFFSET + 8].copy_from_slice(&value.to_le_bytes());
            }

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

        #[cfg(feature = "archive")]
        impl<T> serde::Serialize for $name<T> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.bytes.serialize(serializer)
            }
        }

        #[cfg(feature = "archive")]
        impl<'de, T> serde::Deserialize<'de> for $name<T> {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Ok(Self { bytes: <[u8; $len]>::deserialize(deserializer)?, marker: PhantomData })
            }
        }
    };
}

define_operand!(Operand64, 8);
define_operand!(Operand128, 16);

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

macro_rules! field_ty {
    (u8) => { u8 }; (u16) => { u16 }; (u32) => { u32 }; (u64) => { u64 };
    (i32) => { i32 }; (i64) => { i64 }; (idx128) => { Operand128Idx<[u8; 16]> };
}
macro_rules! field_size {
    (u8) => {{ 1 }};
    (u16) => {{ 2 }};
    (u32) => {{ 4 }};
    (u64) => {{ 8 }};
    (i32) => {{ 4 }};
    (i64) => {{ 8 }};
    (idx128) => {{ 4 }};
}

macro_rules! read_field {
    ($self:ident, u8, $offset:expr) => {{ $self.bytes[$offset] }};
    ($self:ident, u16, $offset:expr) => {{ u16::from_le_bytes($crate::operands::read_bytes!($self, $offset, 2)) }};
    ($self:ident, u32, $offset:expr) => {{ u32::from_le_bytes($crate::operands::read_bytes!($self, $offset, 4)) }};
    ($self:ident, u64, $offset:expr) => {{ u64::from_le_bytes($crate::operands::read_bytes!($self, $offset, 8)) }};
    ($self:ident, i32, $offset:expr) => {{ read_field!($self, u32, $offset) as i32 }};
    ($self:ident, i64, $offset:expr) => {{ read_field!($self, u64, $offset) as i64 }};
    ($self:ident, idx128, $offset:expr) => {{ Operand128Idx::new(read_field!($self, u32, $offset)) }};
}

macro_rules! read_bytes {
    ($self:ident, $offset:expr, $len:literal) => {{
        match <[u8; $len]>::try_from(&$self.bytes[$offset..$offset + $len]) {
            Ok(bytes) => bytes,
            Err(_) => {
                core::hint::cold_path();
                unreachable!("invalid operand field read")
            }
        }
    }};
}

macro_rules! write_field {
    ($self:ident, u8, $offset:expr, $value:expr) => {{ $self.write_u8::<{ $offset }>($value) }};
    ($self:ident, u16, $offset:expr, $value:expr) => {{ $self.write_u16::<{ $offset }>($value) }};
    ($self:ident, u32, $offset:expr, $value:expr) => {{ $self.write_u32::<{ $offset }>($value) }};
    ($self:ident, u64, $offset:expr, $value:expr) => {{ $self.write_u64::<{ $offset }>($value) }};
    ($self:ident, i32, $offset:expr, $value:expr) => {{ $self.write_u32::<{ $offset }>($value as u32) }};
    ($self:ident, i64, $offset:expr, $value:expr) => {{ $self.write_u64::<{ $offset }>($value as u64) }};
    ($self:ident, idx128, $offset:expr, $value:expr) => {{ $self.write_u32::<{ $offset }>($value.index) }};
}

pub(crate) use {read_bytes, read_field, write_field};

impl<T> Operand64<T> {
    /// Returns the jump target stored at the start of a retargetable operand.
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
}

impl<T> Operand128<T> {
    /// Returns the jump target stored at the start of a retargetable operand.
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
}

macro_rules! packed_layout {
    ($operand:ident, $lane:ident, $field:tt) => {
        impl $operand<field_ty!($field)> {
            #[inline]
            pub fn new(value: field_ty!($field)) -> Self {
                let mut operand = Self::default();
                write_field!(operand, $field, 0, value);
                operand
            }

            #[inline(always)]
            pub fn value(&self) -> field_ty!($field) {
                read_field!(self, $field, 0)
            }
        }
    };
    ($operand:ident, $lane:ident, $a:tt, $b:tt) => {
        impl $operand<(field_ty!($a), field_ty!($b))> {
            #[inline]
            pub fn new(a: field_ty!($a), b: field_ty!($b)) -> Self {
                let mut operand = Self::default();
                write_field!(operand, $a, 0, a);
                write_field!(operand, $b, field_size!($a), b);
                operand
            }

            #[inline(always)]
            pub fn a(&self) -> field_ty!($a) {
                read_field!(self, $a, 0)
            }

            #[inline(always)]
            pub fn b(&self) -> field_ty!($b) {
                read_field!(self, $b, field_size!($a))
            }
        }
    };
    ($operand:ident, $lane:ident, $a:tt, $b:tt, $c:tt) => {
        impl $operand<(field_ty!($a), field_ty!($b), field_ty!($c))> {
            #[inline]
            pub fn new(a: field_ty!($a), b: field_ty!($b), c: field_ty!($c)) -> Self {
                let mut operand = Self::default();
                write_field!(operand, $a, 0, a);
                write_field!(operand, $b, field_size!($a), b);
                write_field!(operand, $c, field_size!($a) + field_size!($b), c);
                operand
            }

            #[inline(always)]
            pub fn a(&self) -> field_ty!($a) {
                read_field!(self, $a, 0)
            }

            #[inline(always)]
            pub fn b(&self) -> field_ty!($b) {
                read_field!(self, $b, field_size!($a))
            }

            #[inline(always)]
            pub fn c(&self) -> field_ty!($c) {
                read_field!(self, $c, field_size!($a) + field_size!($b))
            }
        }
    };
}

packed_layout!(Operand64, operands64, i64);
packed_layout!(Operand64, operands64, u32, u32);
packed_layout!(Operand64, operands64, u16, u32);
packed_layout!(Operand64, operands64, u32, u16);
packed_layout!(Operand64, operands64, u16, u16, u16);
packed_layout!(Operand64, operands64, u16, u16, u32);
packed_layout!(Operand64, operands64, u32, i32);
packed_layout!(Operand64, operands64, u32, u16, u16);
packed_layout!(Operand64, operands64, u16, idx128);
packed_layout!(Operand64, operands64, u32, idx128);
packed_layout!(Operand64, operands64, u16, u16, idx128);
packed_layout!(Operand128, operands128, u16, u64);
packed_layout!(Operand128, operands128, u32, u64);
packed_layout!(Operand128, operands128, u16, u16, u64);
packed_layout!(Operand128, operands128, u32, i64);
packed_layout!(Operand128, operands128, u32, i32, u16);

impl Operand128<[u8; 16]> {
    #[inline]
    pub fn new(value: [u8; 16]) -> Self {
        Self { bytes: value, marker: PhantomData }
    }

    #[inline(always)]
    pub fn value(&self) -> [u8; 16] {
        self.bytes
    }
}

const _: () = {
    assert!(core::mem::size_of::<Operand64Idx<i64>>() == 4);
    assert!(core::mem::size_of::<Operand64>() == 8);
    assert!(core::mem::size_of::<Operand128>() == 16);
};
