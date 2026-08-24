/// An abstract WebAssembly heap type.
///
/// This contains exactly the abstract heap types in core Wasm 3.0.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-absheaptype>
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
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#syntax-reftype>
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
