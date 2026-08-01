use core::fmt::Debug;

use crate::{ConstInstruction, RefType, RefValue};

/// A WebAssembly value.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#value-types>
#[derive(Clone, Copy, PartialEq)]
pub enum WasmValue {
    // Num types
    /// A 32-bit integer
    I32(i32),
    /// A 64-bit integer
    I64(i64),
    /// A 32-bit float
    F32(f32),
    /// A 64-bit float
    F64(f64),
    /// A 128-bit vector
    V128([u8; 16]),
    /// A reference type
    Ref(RefValue),
}

impl WasmValue {
    pub const REF_NULL: Self = Self::Ref(RefValue::Null);
}

impl Debug for WasmValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::I32(i) => write!(f, "i32({i})"),
            Self::I64(i) => write!(f, "i64({i})"),
            Self::F32(i) => write!(f, "f32({i})"),
            Self::F64(i) => write!(f, "f64({i})"),
            Self::V128(i) => write!(f, "v128({i:?})"),
            #[cfg(feature = "debug")]
            Self::Ref(i) => write!(f, "ref({i:?})"),
            #[cfg(not(feature = "debug"))]
            Self::Ref(_) => write!(f, "ref(...)"),
        }
    }
}

impl WasmValue {
    #[inline]
    /// Get the matching [`ConstInstruction`] for this value.
    pub fn const_instr(&self) -> alloc::boxed::Box<[ConstInstruction]> {
        alloc::boxed::Box::new([match self {
            Self::I32(i) => ConstInstruction::I32Const(*i),
            Self::I64(i) => ConstInstruction::I64Const(*i),
            Self::F32(i) => ConstInstruction::F32Const(*i),
            Self::F64(i) => ConstInstruction::F64Const(*i),
            Self::V128(i) => ConstInstruction::V128Const(*i),
            Self::Ref(i) => ConstInstruction::Ref(*i),
        }])
    }

    #[inline]
    /// Get the default value for a given type.
    pub const fn default_for(ty: WasmType) -> Option<Self> {
        match ty {
            WasmType::I32 => Some(Self::I32(0)),
            WasmType::I64 => Some(Self::I64(0)),
            WasmType::F32 => Some(Self::F32(0.0)),
            WasmType::F64 => Some(Self::F64(0.0)),
            WasmType::V128 => Some(Self::V128([0; 16])),
            WasmType::Ref(ty) if ty.is_nullable() => Some(Self::REF_NULL),
            WasmType::Ref(_) => None,
        }
    }

    #[inline]
    /// Check if two values are equal, ignoring differences in NaN values.
    pub fn eq_loose(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::I32(a), Self::I32(b)) => a == b,
            (Self::I64(a), Self::I64(b)) => a == b,
            (Self::V128(a), Self::V128(b)) => a == b || Self::v128_nan_eq(*a, *b),
            (Self::Ref(a), Self::Ref(b)) => a == b,
            (Self::F32(a), Self::F32(b)) => a.is_nan() && b.is_nan() || a.to_bits() == b.to_bits(),
            (Self::F64(a), Self::F64(b)) => a.is_nan() && b.is_nan() || a.to_bits() == b.to_bits(),
            _ => false,
        }
    }

    fn v128_nan_eq(a: [u8; 16], b: [u8; 16]) -> bool {
        let a_f32x4: [f32; 4] = [
            f32::from_le_bytes([a[0], a[1], a[2], a[3]]),
            f32::from_le_bytes([a[4], a[5], a[6], a[7]]),
            f32::from_le_bytes([a[8], a[9], a[10], a[11]]),
            f32::from_le_bytes([a[12], a[13], a[14], a[15]]),
        ];
        let b_f32x4: [f32; 4] = [
            f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            f32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            f32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            f32::from_le_bytes([b[12], b[13], b[14], b[15]]),
        ];

        let all_nan_match = a_f32x4.iter().zip(b_f32x4.iter()).all(|(x, y)| {
            if x.is_nan() && y.is_nan() {
                true
            } else if x.is_nan() || y.is_nan() {
                false
            } else {
                x.to_bits() == y.to_bits()
            }
        });

        if all_nan_match && a_f32x4.iter().any(|x| x.is_nan()) {
            return true;
        }

        let a_f64x2: [f64; 2] = [
            f64::from_le_bytes([a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]]),
            f64::from_le_bytes([a[8], a[9], a[10], a[11], a[12], a[13], a[14], a[15]]),
        ];
        let b_f64x2: [f64; 2] = [
            f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
            f64::from_le_bytes([b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]]),
        ];

        a_f64x2.iter().zip(b_f64x2.iter()).all(|(x, y)| {
            if x.is_nan() && y.is_nan() {
                true
            } else if x.is_nan() || y.is_nan() {
                false
            } else {
                x.to_bits() == y.to_bits()
            }
        }) && a_f64x2.iter().any(|x| x.is_nan())
    }
}

/// Type of a WebAssembly value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum WasmType {
    /// A 32-bit integer
    I32,
    /// A 64-bit integer
    I64,
    /// A 32-bit float
    F32,
    /// A 64-bit float
    F64,
    /// A 128-bit vector
    V128,
    /// A reference type
    Ref(RefType),
}

impl WasmType {
    #[inline]
    pub const fn default_value(&self) -> Option<WasmValue> {
        WasmValue::default_for(*self)
    }

    #[inline]
    pub const fn is_simd(&self) -> bool {
        matches!(self, Self::V128)
    }
}

macro_rules! impl_conversion_for_wasmvalue {
    ($($t:ty => $variant:ident, $accessor:ident, $doc:literal);* $(;)?) => {
        impl WasmValue {
            $(
                #[doc = $doc]
                pub const fn $accessor(&self) -> Option<$t> {
                    match self {
                        Self::$variant(value) => Some(*value),
                        _ => None,
                    }
                }
            )*
        }

        $(
            impl From<$t> for WasmValue {
                #[inline]
                fn from(i: $t) -> Self {
                    Self::$variant(i)
                }
            }

            impl TryFrom<WasmValue> for $t {
                type Error = ();

                #[inline]
                fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
                    if let WasmValue::$variant(i) = value { Ok(i) } else { Err(()) }
                }
            }
        )*
    }
}

impl_conversion_for_wasmvalue! {
    i32 => I32, as_i32, "Return the `i32` from a `WasmValue`, if it is an `I32`.";
    i64 => I64, as_i64, "Return the `i64` from a `WasmValue`, if it is an `I64`.";
    f32 => F32, as_f32, "Return the `f32` from a `WasmValue`, if it is a `F32`.";
    f64 => F64, as_f64, "Return the `f64` from a `WasmValue`, if it is a `F64`.";
    [u8; 16] => V128, as_v128, "Return the raw little-endian bytes from a `WasmValue`, if it is a `V128`.";
    RefValue => Ref, as_ref, "Return the `RefValue` from a `WasmValue`, if it is a `Ref`.";
}
