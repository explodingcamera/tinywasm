const HOST_REF_TAG: u32 = 1 << 30;

const fn encode_host_ref(addr: u32) -> Option<u32> {
    if addr >= HOST_REF_TAG - 1 {
        return None;
    }
    Some((addr | HOST_REF_TAG).wrapping_add(1).wrapping_mul(2))
}

/// An abstract WebAssembly heap type.
///
/// This contains exactly the abstract heap types in core Wasm 3.0.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum AbstractHeapType {
    Any,
    Eq,
    I31,
    Struct,
    Array,
    None,

    Func,
    NoFunc,

    Exn,
    NoExn,

    Extern,
    NoExtern,
}

/// A WebAssembly reference type.
///
/// Packed as:
///
/// ```text
/// [nullable:1 concrete:1 payload:30]
/// ```
///
/// For concrete types, `payload` is a module type index before instantiation
/// and a canonical store type address at runtime.
/// Otherwise, it is an [`AbstractHeapType`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct RefType(u32);

#[cfg(feature = "debug")]
impl core::fmt::Debug for RefType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_concrete() {
            write!(f, "concrete({})", self.type_index().unwrap())
        } else {
            write!(f, "abstract({:?})", self.abstract_heap_type().unwrap())
        }
    }
}

impl RefType {
    const NULLABLE: u32 = 1 << 31;
    const CONCRETE: u32 = 1 << 30;
    const PAYLOAD_MASK: u32 = Self::CONCRETE - 1;

    pub const FUNCREF: Self = Self::new_abstract(true, AbstractHeapType::Func);
    pub const EXTERNREF: Self = Self::new_abstract(true, AbstractHeapType::Extern);
    pub const EXNREF: Self = Self::new_abstract(true, AbstractHeapType::Exn);

    #[inline]
    #[doc(hidden)]
    pub const fn to_bits(self) -> u32 {
        self.0
    }

    #[inline]
    pub(crate) const fn from_bits(bits: u32) -> Option<Self> {
        let ty = Self(bits);
        if ty.is_concrete() || ty.abstract_heap_type().is_some() { Some(ty) } else { None }
    }

    #[inline]
    pub const fn new_abstract(nullable: bool, heap_type: AbstractHeapType) -> Self {
        Self((nullable as u32) << 31 | heap_type as u32)
    }

    #[inline]
    pub const fn new_concrete(nullable: bool, type_index: u32) -> Self {
        assert!(type_index <= Self::PAYLOAD_MASK, "type index is too large for a reference type");
        Self(((nullable as u32) << 31) | Self::CONCRETE | type_index)
    }

    #[inline]
    pub const fn is_nullable(self) -> bool {
        self.0 & Self::NULLABLE != 0
    }

    #[inline]
    pub const fn is_concrete(self) -> bool {
        self.0 & Self::CONCRETE != 0
    }

    #[inline]
    pub const fn type_index(self) -> Option<u32> {
        if self.is_concrete() { Some(self.0 & Self::PAYLOAD_MASK) } else { None }
    }

    #[inline]
    pub const fn abstract_heap_type(self) -> Option<AbstractHeapType> {
        if self.is_concrete() {
            return None;
        }

        match self.0 & Self::PAYLOAD_MASK {
            0 => Some(AbstractHeapType::Any),
            1 => Some(AbstractHeapType::Eq),
            2 => Some(AbstractHeapType::I31),
            3 => Some(AbstractHeapType::Struct),
            4 => Some(AbstractHeapType::Array),
            5 => Some(AbstractHeapType::None),
            6 => Some(AbstractHeapType::Func),
            7 => Some(AbstractHeapType::NoFunc),
            8 => Some(AbstractHeapType::Exn),
            9 => Some(AbstractHeapType::NoExn),
            10 => Some(AbstractHeapType::Extern),
            11 => Some(AbstractHeapType::NoExtern),
            _ => None,
        }
    }

    #[inline]
    pub const fn with_nullability(self, nullable: bool) -> Self {
        Self((self.0 & !Self::NULLABLE) | ((nullable as u32) << 31))
    }

    #[inline]
    pub const fn is_func(self) -> bool {
        matches!(self.abstract_heap_type(), Some(AbstractHeapType::Func | AbstractHeapType::NoFunc))
    }

    #[inline]
    pub const fn is_extern(self) -> bool {
        matches!(self.abstract_heap_type(), Some(AbstractHeapType::Extern | AbstractHeapType::NoExtern))
    }

    #[inline]
    pub const fn is_exn(self) -> bool {
        matches!(self.abstract_heap_type(), Some(AbstractHeapType::Exn | AbstractHeapType::NoExn))
    }
}

/// A host-facing WebAssembly reference value.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum RefValue {
    Null,
    Func(FuncRef),
    Extern(ExternRef),
    Any(AnyRef),
    Exn(ExnRef),
}

/// A reference to a function in a store.
///
/// The payload is the function's store-local address.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct FuncRef(u32);

impl FuncRef {
    #[inline]
    pub const fn new(addr: u32) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn addr(self) -> u32 {
        self.0
    }
}

/// An opaque external reference.
///
/// Packed as `[payload:31 i31:1]`. Host addresses use the upper payload
/// category, Store-managed objects use the lower category, and odd values
/// contain an externalized i31.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ExternRef(u32);

impl ExternRef {
    #[inline]
    pub const fn new(addr: u32) -> Self {
        let Some(value) = Self::try_new(addr) else { panic!("external reference address is too large") };
        value
    }

    /// Creates an external reference when `addr` fits the runtime encoding.
    #[inline]
    pub const fn try_new(addr: u32) -> Option<Self> {
        match encode_host_ref(addr) {
            Some(encoded) => Some(Self(encoded)),
            None => None,
        }
    }

    #[doc(hidden)]
    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[doc(hidden)]
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// A reference to an exception in a store.
///
/// The payload is the exception's store-local address.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ExnRef(u32);

impl ExnRef {
    #[inline]
    pub const fn new(addr: u32) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn addr(self) -> u32 {
        self.0
    }
}

/// A WebAssembly `anyref` value.
///
/// Packed as:
///
/// ```text
/// [payload:31 i31:1]
/// ```
///
/// Odd values contain an inline signed i31. Non-zero even values are reserved
/// for store-managed references, and zero is reserved for null by the runtime.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct AnyRef(u32);

impl AnyRef {
    /// Creates a host reference when `addr` fits the runtime encoding.
    #[inline]
    pub const fn from_host(addr: u32) -> Option<Self> {
        match encode_host_ref(addr) {
            Some(encoded) => Some(Self(encoded)),
            None => None,
        }
    }

    #[doc(hidden)]
    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn from_i31(value: i32) -> Option<Self> {
        if value < -(1 << 30) || value >= (1 << 30) {
            return None;
        }

        Some(Self(((value as u32) << 1) | 1))
    }

    pub const fn as_i31(self) -> Option<i32> {
        if self.0 & 1 == 1 { Some((self.0 as i32) >> 1) } else { None }
    }

    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_reference_encoding_is_checked_and_unique() {
        assert_ne!(AnyRef::from_host(0), AnyRef::from_host(1));
        assert!(AnyRef::from_host(HOST_REF_TAG - 2).is_some());
        assert!(AnyRef::from_host(HOST_REF_TAG - 1).is_none());
        assert!(ExternRef::try_new(u32::MAX).is_none());
    }
}
