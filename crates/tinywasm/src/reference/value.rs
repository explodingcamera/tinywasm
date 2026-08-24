use core::fmt::Debug;

use tinywasm_types::{AbstractHeapType, RefType, WasmType};

use crate::interpreter::{RuntimeValue, Value128};
use crate::{AnyRef, ArrayRef, EqRef, ExnRef, ExternRef, FuncRef, I31Ref, RefValue, Result, Store, StructRef};

/// A host-facing WebAssembly value.
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#values>
#[derive(Clone, PartialEq)]
pub enum WasmValue {
    /// A 32-bit integer.
    I32(i32),
    /// A 64-bit integer.
    I64(i64),
    /// A 32-bit float.
    F32(f32),
    /// A 64-bit float.
    F64(f64),
    /// A 128-bit vector.
    V128([u8; 16]),
    /// A reference.
    Ref(RefValue),
}

impl WasmValue {
    pub(crate) fn to_runtime(&self, store: &Store) -> Result<RuntimeValue> {
        Ok(match self {
            Self::I32(value) => RuntimeValue::Value32(*value as u32),
            Self::I64(value) => RuntimeValue::Value64(*value as u64),
            Self::F32(value) => RuntimeValue::Value32(value.to_bits()),
            Self::F64(value) => RuntimeValue::Value64(value.to_bits()),
            Self::V128(value) => RuntimeValue::Value128(Value128(*value)),
            Self::Ref(value) => RuntimeValue::ValueRef(store.encode_ref(value)?),
        })
    }

    /// Returns this value's broad WebAssembly type, or `None` for null.
    pub fn ty(&self) -> Option<WasmType> {
        match self {
            Self::I32(_) => Some(WasmType::I32),
            Self::I64(_) => Some(WasmType::I64),
            Self::F32(_) => Some(WasmType::F32),
            Self::F64(_) => Some(WasmType::F64),
            Self::V128(_) => Some(WasmType::V128),
            Self::Ref(RefValue::Null) => None,
            Self::Ref(RefValue::Func(_)) => Some(WasmType::Ref(RefType::FUNCREF)),
            Self::Ref(RefValue::Extern(_)) => Some(WasmType::Ref(RefType::EXTERNREF)),
            Self::Ref(RefValue::Exn(_)) => Some(WasmType::Ref(RefType::EXNREF)),
            Self::Ref(RefValue::Any(_)) => Some(WasmType::Ref(RefType::new_abstract(true, AbstractHeapType::Any))),
        }
    }

    /// Returns whether this value matches a broad or abstract type.
    ///
    /// A non-null value does not match a concrete reference type because
    /// concrete types require Store-specific subtype information.
    pub fn matches_type(&self, ty: WasmType) -> bool {
        match (self, ty) {
            (Self::I32(_), WasmType::I32)
            | (Self::I64(_), WasmType::I64)
            | (Self::F32(_), WasmType::F32)
            | (Self::F64(_), WasmType::F64)
            | (Self::V128(_), WasmType::V128) => true,
            (Self::Ref(RefValue::Null), WasmType::Ref(ty)) => ty.is_nullable(),
            (Self::Ref(RefValue::Func(_)), WasmType::Ref(ty)) => {
                ty.abstract_heap_type() == Some(AbstractHeapType::Func)
            }
            (Self::Ref(RefValue::Extern(_)), WasmType::Ref(ty)) => {
                ty.abstract_heap_type() == Some(AbstractHeapType::Extern)
            }
            (Self::Ref(RefValue::Exn(_)), WasmType::Ref(ty)) => ty.abstract_heap_type() == Some(AbstractHeapType::Exn),
            (Self::Ref(RefValue::Any(value)), WasmType::Ref(ty)) => match ty.abstract_heap_type() {
                Some(AbstractHeapType::Any) => true,
                Some(AbstractHeapType::Eq) => value.as_eq().is_some(),
                Some(AbstractHeapType::I31) => value.as_i31().is_some(),
                Some(AbstractHeapType::Struct) => value.as_struct().is_some(),
                Some(AbstractHeapType::Array) => value.as_array().is_some(),
                _ => false,
            },
            _ => false,
        }
    }

    /// Returns the default value for `ty`, or `None` for a non-null reference type.
    pub fn default_for(ty: WasmType) -> Option<Self> {
        match ty {
            WasmType::I32 => Some(Self::I32(0)),
            WasmType::I64 => Some(Self::I64(0)),
            WasmType::F32 => Some(Self::F32(0.0)),
            WasmType::F64 => Some(Self::F64(0.0)),
            WasmType::V128 => Some(Self::V128([0; 16])),
            WasmType::Ref(ty) if ty.is_nullable() => Some(Self::Ref(RefValue::Null)),
            WasmType::Ref(_) => None,
        }
    }

    /// Compares values while treating NaN values with different payloads as equal.
    pub fn eq_loose(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::F32(a), Self::F32(b)) => a.is_nan() && b.is_nan() || a.to_bits() == b.to_bits(),
            (Self::F64(a), Self::F64(b)) => a.is_nan() && b.is_nan() || a.to_bits() == b.to_bits(),
            (Self::V128(a), Self::V128(b)) => a == b || vector_nan_eq(*a, *b),
            _ => self == other,
        }
    }
}

fn vector_nan_eq(a: [u8; 16], b: [u8; 16]) -> bool {
    let f32_equal = a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0).all(|(a, b)| {
        let a = f32::from_le_bytes(*a);
        let b = f32::from_le_bytes(*b);
        a.is_nan() && b.is_nan() || a.to_bits() == b.to_bits()
    });
    if f32_equal && a.as_chunks::<4>().0.iter().any(|v| f32::from_le_bytes(*v).is_nan()) {
        return true;
    }
    a.as_chunks::<8>().0.iter().zip(b.as_chunks::<8>().0).all(|(a, b)| {
        let a = f64::from_le_bytes(*a);
        let b = f64::from_le_bytes(*b);
        a.is_nan() && b.is_nan() || a.to_bits() == b.to_bits()
    }) && a.as_chunks::<8>().0.iter().any(|v| f64::from_le_bytes(*v).is_nan())
}

impl Debug for WasmValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::I32(value) => write!(f, "i32({value})"),
            Self::I64(value) => write!(f, "i64({value})"),
            Self::F32(value) => write!(f, "f32({value})"),
            Self::F64(value) => write!(f, "f64({value})"),
            Self::V128(value) => write!(f, "v128({value:?})"),
            #[cfg(feature = "debug")]
            Self::Ref(value) => write!(f, "ref({value:?})"),
            #[cfg(not(feature = "debug"))]
            Self::Ref(_) => f.write_str("ref(..)"),
        }
    }
}

macro_rules! value_conversions {
    ($($ty:ty => $variant:ident, $method:ident);* $(;)?) => {$(
        impl WasmValue {
            #[doc = concat!("Returns the contained `", stringify!($ty), "` value.")]
            pub fn $method(&self) -> Option<$ty> {
                if let Self::$variant(value) = self { Some(value.clone()) } else { None }
            }
        }
        impl From<$ty> for WasmValue { fn from(value: $ty) -> Self { Self::$variant(value) } }
        impl TryFrom<WasmValue> for $ty {
            type Error = ();
            fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
                if let WasmValue::$variant(value) = value { Ok(value) } else { Err(()) }
            }
        }
    )*};
}

value_conversions! {
    i32 => I32, as_i32;
    i64 => I64, as_i64;
    f32 => F32, as_f32;
    f64 => F64, as_f64;
    [u8; 16] => V128, as_v128;
}

impl WasmValue {
    /// Returns the contained reference value.
    pub const fn as_ref(&self) -> Option<&RefValue> {
        if let Self::Ref(value) = self { Some(value) } else { None }
    }
}

impl From<RefValue> for WasmValue {
    fn from(value: RefValue) -> Self {
        Self::Ref(value)
    }
}

impl TryFrom<WasmValue> for RefValue {
    type Error = ();

    fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
        if let WasmValue::Ref(value) = value { Ok(value) } else { Err(()) }
    }
}

macro_rules! ref_conversions {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(
        impl From<$ty> for WasmValue { fn from(value: $ty) -> Self { Self::Ref(RefValue::$variant(value)) } }
        impl TryFrom<WasmValue> for $ty {
            type Error = ();
            fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
                if let WasmValue::Ref(RefValue::$variant(value)) = value { Ok(value) } else { Err(()) }
            }
        }
    )*};
}

ref_conversions!(FuncRef => Func, ExternRef => Extern, AnyRef => Any, ExnRef => Exn);

macro_rules! any_ref_conversions {
    ($($ty:ty => $cast:ident),* $(,)?) => {$(
        impl From<$ty> for WasmValue { fn from(value: $ty) -> Self { Self::Ref(RefValue::Any(value.to_any())) } }
        impl TryFrom<WasmValue> for $ty {
            type Error = ();
            fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
                let WasmValue::Ref(RefValue::Any(value)) = value else { return Err(()) };
                value.$cast().ok_or(())
            }
        }
    )*};
}

any_ref_conversions!(EqRef => as_eq, I31Ref => as_i31, StructRef => as_struct, ArrayRef => as_array);

macro_rules! nullable_conversions {
    ($($ty:ty),* $(,)?) => {$(
        impl From<Option<$ty>> for WasmValue {
            fn from(value: Option<$ty>) -> Self { value.map(Into::into).unwrap_or(Self::Ref(RefValue::Null)) }
        }
        impl TryFrom<WasmValue> for Option<$ty> {
            type Error = ();
            fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
                if matches!(value, WasmValue::Ref(RefValue::Null)) { Ok(None) } else { <$ty>::try_from(value).map(Some) }
            }
        }
    )*};
}

nullable_conversions!(FuncRef, AnyRef, EqRef, I31Ref, StructRef, ArrayRef, ExternRef, ExnRef);
