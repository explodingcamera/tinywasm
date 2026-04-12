use core::fmt::Debug;

use crate::{ConstInstruction, ExternAddr, FuncAddr};

/// A WebAssembly value.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#value-types>
#[derive(Clone, Copy, PartialEq)]
pub enum WasmValue {
    // Num types
    /// A 32-bit integer.
    I32(i32),
    /// A 64-bit integer.
    I64(i64),
    /// A 32-bit float.
    F32(f32),
    /// A 64-bit float.
    F64(f64),
    // /// A 128-bit vector
    V128(i128),

    RefExtern(ExternRef),
    RefFunc(FuncRef),
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
            Self::RefExtern(i) => write!(f, "ref({i:?})"),
            #[cfg(feature = "debug")]
            Self::RefFunc(i) => write!(f, "func({i:?})"),
            #[cfg(not(feature = "debug"))]
            Self::RefExtern(_) => write!(f, "ref()"),
            #[cfg(not(feature = "debug"))]
            Self::RefFunc(_) => write!(f, "func()"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ExternRef(Option<ExternAddr>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct FuncRef(Option<FuncAddr>);

#[cfg(feature = "debug")]
impl Debug for ExternRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(addr) => write!(f, "extern({addr:?})"),
            None => write!(f, "extern(null)"),
        }
    }
}

#[cfg(feature = "debug")]
impl Debug for FuncRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(addr) => write!(f, "func({addr:?})"),
            None => write!(f, "func(null)"),
        }
    }
}

impl FuncRef {
    /// Create a new `FuncRef` from a `FuncAddr`.
    /// Should only be used by the runtime.
    #[doc(hidden)]
    #[inline]
    pub const fn new(addr: Option<FuncAddr>) -> Self {
        Self(addr)
    }

    /// Create a null `FuncRef`.
    #[inline]
    pub const fn null() -> Self {
        Self(None)
    }

    /// Check if the `FuncRef` is null.
    #[inline]
    pub const fn is_null(&self) -> bool {
        self.0.is_none()
    }

    /// Get the `FuncAddr` from the `FuncRef`.
    #[inline]
    pub const fn addr(&self) -> Option<FuncAddr> {
        self.0
    }
}

impl ExternRef {
    /// Create a new `ExternRef` from an `ExternAddr`.
    /// Should only be used by the runtime.
    #[doc(hidden)]
    #[inline]
    pub const fn new(addr: Option<ExternAddr>) -> Self {
        Self(addr)
    }

    /// Create a null `ExternRef`.
    #[inline]
    pub const fn null() -> Self {
        Self(None)
    }

    /// Check if the `ExternRef` is null.
    #[inline]
    pub const fn is_null(&self) -> bool {
        self.0.is_none()
    }

    /// Get the `ExternAddr` from the `ExternRef`.
    #[inline]
    pub const fn addr(&self) -> Option<ExternAddr> {
        self.0
    }
}

impl WasmValue {
    #[doc(hidden)]
    #[inline]
    pub fn const_instr(&self) -> alloc::boxed::Box<[ConstInstruction]> {
        alloc::boxed::Box::new([match self {
            Self::I32(i) => ConstInstruction::I32Const(*i),
            Self::I64(i) => ConstInstruction::I64Const(*i),
            Self::F32(i) => ConstInstruction::F32Const(*i),
            Self::F64(i) => ConstInstruction::F64Const(*i),
            Self::V128(i) => ConstInstruction::V128Const(*i),
            Self::RefFunc(i) => ConstInstruction::RefFunc(i.addr()),
            Self::RefExtern(i) => ConstInstruction::RefExtern(i.addr()),
        }])
    }

    /// Get the default value for a given type.
    #[inline]
    pub const fn default_for(ty: ValType) -> Self {
        match ty {
            ValType::I32 => Self::I32(0),
            ValType::I64 => Self::I64(0),
            ValType::F32 => Self::F32(0.0),
            ValType::F64 => Self::F64(0.0),
            ValType::V128 => Self::V128(0),
            ValType::RefFunc => Self::RefFunc(FuncRef::null()),
            ValType::RefExtern => Self::RefExtern(ExternRef::null()),
        }
    }

    /// Check if two values are equal, ignoring differences in NaN values.
    #[inline]
    pub fn eq_loose(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::I32(a), Self::I32(b)) => a == b,
            (Self::I64(a), Self::I64(b)) => a == b,
            (Self::V128(a), Self::V128(b)) => {
                let a_bytes = a.to_le_bytes();
                let b_bytes = b.to_le_bytes();
                a_bytes == b_bytes || Self::v128_nan_eq(a_bytes, b_bytes)
            }
            (Self::RefExtern(addr), Self::RefExtern(addr2)) => addr == addr2,
            (Self::RefFunc(addr), Self::RefFunc(addr2)) => addr == addr2,
            (Self::F32(a), Self::F32(b)) => {
                if a.is_nan() && b.is_nan() {
                    true
                } else {
                    a.to_bits() == b.to_bits()
                }
            }
            (Self::F64(a), Self::F64(b)) => {
                if a.is_nan() && b.is_nan() {
                    true
                } else {
                    a.to_bits() == b.to_bits()
                }
            }
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

    #[doc(hidden)]
    pub const fn as_i32(&self) -> Option<i32> {
        match self {
            Self::I32(i) => Some(*i),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::I64(i) => Some(*i),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn as_f32(&self) -> Option<f32> {
        match self {
            Self::F32(i) => Some(*i),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(i) => Some(*i),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn as_v128(&self) -> Option<i128> {
        match self {
            Self::V128(i) => Some(*i),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn as_ref_extern(&self) -> Option<ExternRef> {
        match self {
            Self::RefExtern(ref_extern) => Some(*ref_extern),
            _ => None,
        }
    }

    #[doc(hidden)]
    pub const fn as_ref_func(&self) -> Option<FuncRef> {
        match self {
            Self::RefFunc(ref_func) => Some(*ref_func),
            _ => None,
        }
    }
}

impl WasmValue {
    /// Get the type of a [`WasmValue`]
    #[inline]
    pub const fn val_type(&self) -> ValType {
        match self {
            Self::I32(_) => ValType::I32,
            Self::I64(_) => ValType::I64,
            Self::F32(_) => ValType::F32,
            Self::F64(_) => ValType::F64,
            Self::V128(_) => ValType::V128,
            Self::RefExtern(_) => ValType::RefExtern,
            Self::RefFunc(_) => ValType::RefFunc,
        }
    }
}

/// Type of a WebAssembly value.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
#[cfg_attr(feature = "archive", derive(serde::Serialize, serde::Deserialize))]
pub enum ValType {
    /// A 32-bit integer.
    I32,
    /// A 64-bit integer.
    I64,
    /// A 32-bit float.
    F32,
    /// A 64-bit float.
    F64,
    /// A 128-bit vector
    V128,
    /// A reference to a function.
    RefFunc,
    /// A reference to an external value.
    RefExtern,
}

impl ValType {
    #[inline]
    pub const fn default_value(&self) -> WasmValue {
        WasmValue::default_for(*self)
    }

    #[doc(hidden)]
    #[inline]
    pub const fn is_simd(&self) -> bool {
        matches!(self, Self::V128)
    }
}

macro_rules! impl_conversion_for_wasmvalue {
    ($($t:ty => $variant:ident),*) => {
        $(
            // Implementing From<$t> for WasmValue
            impl From<$t> for WasmValue {
                #[inline]
                fn from(i: $t) -> Self {
                    Self::$variant(i)
                }
            }

            // Implementing TryFrom<WasmValue> for $t
            impl TryFrom<WasmValue> for $t {
                type Error = ();

                #[inline]
                fn try_from(value: WasmValue) -> Result<Self, Self::Error> {
                    if let WasmValue::$variant(i) = value {
                        Ok(i)
                    } else {
                        Err(())
                    }
                }
            }
        )*
    }
}

impl_conversion_for_wasmvalue! { i32 => I32, i64 => I64, f32 => F32, f64 => F64, i128 => V128, ExternRef => RefExtern, FuncRef => RefFunc }
