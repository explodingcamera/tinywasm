use core::fmt::Debug;
use tinywasm_types::{ValType, WasmValue};

/// A raw wasm value.
///
/// This is the internal representation of all wasm values
///
/// See [`WasmValue`] for the public representation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct RawWasmValue(u64);

impl Debug for RawWasmValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RawWasmValue({})", self.0 as i64) // cast to i64 so at least negative numbers for i32 and i64 are printed correctly
    }
}

impl RawWasmValue {
    #[inline(always)]
    pub fn raw_value(&self) -> u64 {
        self.0
    }

    #[inline]
    pub fn attach_type(self, ty: ValType) -> WasmValue {
        match ty {
            ValType::I32 => WasmValue::I32(self.0 as i32),
            ValType::I64 => WasmValue::I64(self.0 as i64),
            ValType::F32 => WasmValue::F32(f32::from_bits(self.0 as u32)),
            ValType::F64 => WasmValue::F64(f64::from_bits(self.0)),
            ValType::RefExtern => {
                if self.0 == -1i64 as u64 {
                    WasmValue::RefNull(ValType::RefExtern)
                } else {
                    WasmValue::RefExtern(self.0 as u32)
                }
            }
            ValType::RefFunc => {
                if self.0 == -1i64 as u64 {
                    WasmValue::RefNull(ValType::RefFunc)
                } else {
                    WasmValue::RefFunc(self.0 as u32)
                }
            }
        }
    }
}

impl From<WasmValue> for RawWasmValue {
    #[inline]
    fn from(v: WasmValue) -> Self {
        match v {
            WasmValue::I32(i) => Self(i as u64),
            WasmValue::I64(i) => Self(i as u64),
            WasmValue::F32(i) => Self(i.to_bits() as u64),
            WasmValue::F64(i) => Self(i.to_bits()),
            WasmValue::RefExtern(v) => Self(v as i64 as u64),
            WasmValue::RefFunc(v) => Self(v as i64 as u64),
            WasmValue::RefNull(_) => Self(-1i64 as u64),
        }
    }
}

macro_rules! impl_from_raw_wasm_value {
    ($type:ty, $to_raw:expr, $from_raw:expr) => {
        // Implement From<$type> for RawWasmValue
        impl From<$type> for RawWasmValue {
            #[inline]
            fn from(value: $type) -> Self {
                #[allow(clippy::redundant_closure_call)] // the comiler will figure it out :)
                Self($to_raw(value))
            }
        }

        // Implement From<RawWasmValue> for $type
        impl From<RawWasmValue> for $type {
            #[inline]
            fn from(value: RawWasmValue) -> Self {
                #[allow(clippy::redundant_closure_call)] // the comiler will figure it out :)
                $from_raw(value.0)
            }
        }
    };
}

impl_from_raw_wasm_value!(i32, |x| x as u64, |x| x as i32);
impl_from_raw_wasm_value!(i64, |x| x as u64, |x| x as i64);
impl_from_raw_wasm_value!(f32, |x| f32::to_bits(x) as u64, |x| f32::from_bits(x as u32));
impl_from_raw_wasm_value!(f64, f64::to_bits, f64::from_bits);

// used for memory load/store
impl_from_raw_wasm_value!(i8, |x| x as u64, |x| x as i8);
impl_from_raw_wasm_value!(i16, |x| x as u64, |x| x as i16);
impl_from_raw_wasm_value!(u32, |x| x as u64, |x| x as u32);
impl_from_raw_wasm_value!(u64, |x| x, |x| x);
