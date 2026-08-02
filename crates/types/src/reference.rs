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
/// For concrete types, `payload` is a module type index.
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
    pub const fn new_abstract(nullable: bool, heap_type: AbstractHeapType) -> Self {
        Self((nullable as u32) << 31 | heap_type as u32)
    }

    #[inline]
    pub const fn new_concrete(nullable: bool, type_index: u32) -> Option<Self> {
        if type_index <= Self::PAYLOAD_MASK {
            Some(Self(((nullable as u32) << 31) | Self::CONCRETE | type_index))
        } else {
            None
        }
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
}

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

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ExternRef(u32);

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct ExnRef(u32);

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub struct AnyRef(u32);

impl AnyRef {
    // Odd values are inline i31s.
    // Even values are GC object handles.

    pub const fn from_i31(value: i32) -> Option<Self> {
        if value < -(1 << 30) || value >= (1 << 30) {
            return None;
        }

        Some(Self(((value as u32) << 1) | 1))
    }

    pub const fn as_i31(self) -> Option<i32> {
        if self.0 & 1 == 1 { Some((self.0 as i32) >> 1) } else { None }
    }

    pub const fn from_gc_addr(addr: u32) -> Option<Self> {
        match addr.checked_add(1) {
            Some(raw) => match raw.checked_mul(2) {
                Some(raw) => Some(Self(raw)),
                None => None,
            },
            None => None,
        }
    }

    pub const fn gc_addr(self) -> Option<u32> {
        if self.0 != 0 && self.0 & 1 == 0 { Some(self.0 / 2 - 1) } else { None }
    }
}
