use core::fmt::Debug;

use crate::{ConstInstruction, ExternAddr, FuncAddr};

/// A WebAssembly value.
///
/// See <https://webassembly.github.io/spec/core/syntax/types.html#value-types>
#[derive(Clone, Copy)]
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
    // V128(u128),
    RefExtern(ExternAddr),
    RefFunc(FuncAddr),
    RefNull(ValType),
}

impl WasmValue {
    #[inline]
    pub fn const_instr(&self) -> ConstInstruction {
        match self {
            Self::I32(i) => ConstInstruction::I32Const(*i),
            Self::I64(i) => ConstInstruction::I64Const(*i),
            Self::F32(i) => ConstInstruction::F32Const(*i),
            Self::F64(i) => ConstInstruction::F64Const(*i),

            Self::RefFunc(i) => ConstInstruction::RefFunc(*i),
            Self::RefNull(ty) => ConstInstruction::RefNull(*ty),

            // Self::RefExtern(addr) => ConstInstruction::RefExtern(*addr),
            _ => unimplemented!("no const_instr for {:?}", self),
        }
    }

    /// Get the default value for a given type.
    #[inline]
    pub fn default_for(ty: ValType) -> Self {
        match ty {
            ValType::I32 => Self::I32(0),
            ValType::I64 => Self::I64(0),
            ValType::F32 => Self::F32(0.0),
            ValType::F64 => Self::F64(0.0),
            // ValType::V128 => Self::V128(0),
            ValType::RefFunc => Self::RefNull(ValType::RefFunc),
            ValType::RefExtern => Self::RefNull(ValType::RefExtern),
        }
    }

    #[inline]
    pub fn eq_loose(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::I32(a), Self::I32(b)) => a == b,
            (Self::I64(a), Self::I64(b)) => a == b,
            (Self::RefNull(v), Self::RefNull(v2)) => v == v2,
            (Self::RefExtern(addr), Self::RefExtern(addr2)) => addr == addr2,
            (Self::RefFunc(addr), Self::RefFunc(addr2)) => addr == addr2,
            (Self::F32(a), Self::F32(b)) => {
                if a.is_nan() && b.is_nan() {
                    true // Both are NaN, treat them as equal
                } else {
                    a.to_bits() == b.to_bits()
                }
            }
            (Self::F64(a), Self::F64(b)) => {
                if a.is_nan() && b.is_nan() {
                    true // Both are NaN, treat them as equal
                } else {
                    a.to_bits() == b.to_bits()
                }
            }
            _ => false,
        }
    }
}

#[cold]
fn cold() {}

impl Debug for WasmValue {
    fn fmt(&self, f: &mut alloc::fmt::Formatter<'_>) -> alloc::fmt::Result {
        match self {
            WasmValue::I32(i) => write!(f, "i32({})", i),
            WasmValue::I64(i) => write!(f, "i64({})", i),
            WasmValue::F32(i) => write!(f, "f32({})", i),
            WasmValue::F64(i) => write!(f, "f64({})", i),
            // WasmValue::V128(i) => write!(f, "v128.half({:?})", i),
            WasmValue::RefExtern(addr) => write!(f, "ref.extern({:?})", addr),
            WasmValue::RefFunc(addr) => write!(f, "ref.func({:?})", addr),
            WasmValue::RefNull(ty) => write!(f, "ref.null({:?})", ty),
        }
    }
}

impl WasmValue {
    /// Get the type of a [`WasmValue`]
    #[inline]
    pub fn val_type(&self) -> ValType {
        match self {
            Self::I32(_) => ValType::I32,
            Self::I64(_) => ValType::I64,
            Self::F32(_) => ValType::F32,
            Self::F64(_) => ValType::F64,
            // Self::V128(_) => ValType::V128,
            Self::RefExtern(_) => ValType::RefExtern,
            Self::RefFunc(_) => ValType::RefFunc,
            Self::RefNull(ty) => *ty,
        }
    }
}

/// Type of a WebAssembly value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "archive", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize), archive(check_bytes))]
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
    // V128,
    /// A reference to a function.
    RefFunc,
    /// A reference to an external value.
    RefExtern,
}

impl ValType {
    #[inline]
    pub fn default_value(&self) -> WasmValue {
        WasmValue::default_for(*self)
    }

    pub(crate) fn to_byte(&self) -> u8 {
        match self {
            ValType::I32 => 0x7F,
            ValType::I64 => 0x7E,
            ValType::F32 => 0x7D,
            ValType::F64 => 0x7C,
            ValType::RefFunc => 0x70,
            ValType::RefExtern => 0x6F,
        }
    }

    pub(crate) fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x7F => Some(ValType::I32),
            0x7E => Some(ValType::I64),
            0x7D => Some(ValType::F32),
            0x7C => Some(ValType::F64),
            0x70 => Some(ValType::RefFunc),
            0x6F => Some(ValType::RefExtern),
            _ => None,
        }
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
                        cold();
                        Err(())
                    }
                }
            }
        )*
    }
}

impl_conversion_for_wasmvalue! {
    i32 => I32,
    i64 => I64,
    f32 => F32,
    f64 => F64
}
