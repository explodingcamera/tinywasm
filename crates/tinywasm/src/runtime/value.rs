use core::fmt::Debug;
use tinywasm_types::{ValType, WasmValue};

/// A raw wasm value.
///
/// This is the internal representation of all wasm values
///
/// See [`WasmValue`] for the public representation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
// pub struct RawWasmValue([u8; 16]);
pub struct RawWasmValue([u8; 8]);

impl Debug for RawWasmValue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RawWasmValue({})", 0)
    }
}

impl RawWasmValue {
    #[inline(always)]
    pub fn raw_value(&self) -> [u8; 8] {
        self.0
    }

    #[inline]
    pub fn attach_type(self, ty: ValType) -> WasmValue {
        match ty {
            ValType::I32 => WasmValue::I32(self.into()),
            ValType::I64 => WasmValue::I64(self.into()),
            ValType::F32 => WasmValue::F32(f32::from_bits(self.into())),
            ValType::F64 => WasmValue::F64(f64::from_bits(self.into())),
            // ValType::V128 => WasmValue::V128(self.into()),
            ValType::RefExtern => {
                let val: i64 = self.into();
                if val < 0 {
                    WasmValue::RefNull(ValType::RefExtern)
                } else {
                    WasmValue::RefExtern(val as u32)
                }
            }
            ValType::RefFunc => {
                let val: i64 = self.into();
                if val < 0 {
                    WasmValue::RefNull(ValType::RefFunc)
                } else {
                    WasmValue::RefFunc(val as u32)
                }
            }
        }
    }
}

impl From<WasmValue> for RawWasmValue {
    #[inline]
    fn from(v: WasmValue) -> Self {
        match v {
            WasmValue::I32(i) => Self::from(i),
            WasmValue::I64(i) => Self::from(i),
            WasmValue::F32(i) => Self::from(i),
            WasmValue::F64(i) => Self::from(i),
            // WasmValue::V128(i) => Self::from(i),
            WasmValue::RefExtern(v) => Self::from(v as i64),
            WasmValue::RefFunc(v) => Self::from(v as i64),
            WasmValue::RefNull(_) => Self::from(-1i64),
        }
    }
}

macro_rules! impl_from_raw_wasm_value {
    ($type:ty, $to_raw:expr, $from_raw:expr) => {
        // Implement From<$type> for RawWasmValue
        impl From<$type> for RawWasmValue {
            #[inline]
            fn from(value: $type) -> Self {
                #[allow(clippy::redundant_closure_call)]
                Self(u64::to_ne_bytes($to_raw(value)))
            }
        }

        // Implement From<RawWasmValue> for $type
        impl From<RawWasmValue> for $type {
            #[inline]
            fn from(value: RawWasmValue) -> Self {
                #[allow(clippy::redundant_closure_call)]
                $from_raw(value.0)
            }
        }
    };
}

type RawValue = u64;
type RawValueRep = [u8; 8];

// This all looks like a lot of extra steps, but the compiler will optimize it all away.
// The `u128` is used to make the conversion easier to write.
impl_from_raw_wasm_value!(i32, |x| x as RawValue, |x: RawValueRep| i32::from_ne_bytes(x[0..4].try_into().unwrap()));
impl_from_raw_wasm_value!(i64, |x| x as RawValue, |x: RawValueRep| i64::from_ne_bytes(x[0..8].try_into().unwrap()));
impl_from_raw_wasm_value!(f32, |x| f32::to_bits(x) as RawValue, |x: RawValueRep| f32::from_bits(u32::from_ne_bytes(
    x[0..4].try_into().unwrap()
)));
impl_from_raw_wasm_value!(f64, |x| f64::to_bits(x) as RawValue, |x: RawValueRep| f64::from_bits(u64::from_ne_bytes(
    x[0..8].try_into().unwrap()
)));

impl_from_raw_wasm_value!(u8, |x| x as RawValue, |x: RawValueRep| u8::from_ne_bytes(x[0..1].try_into().unwrap()));
impl_from_raw_wasm_value!(u16, |x| x as RawValue, |x: RawValueRep| u16::from_ne_bytes(x[0..2].try_into().unwrap()));
impl_from_raw_wasm_value!(u32, |x| x as RawValue, |x: RawValueRep| u32::from_ne_bytes(x[0..4].try_into().unwrap()));
impl_from_raw_wasm_value!(u64, |x| x as RawValue, |x: RawValueRep| u64::from_ne_bytes(x[0..8].try_into().unwrap()));
// impl_from_raw_wasm_value!(u128, |x| x, |x: RawValueRep| RawValue::from_ne_bytes(x));

impl_from_raw_wasm_value!(i8, |x| x as RawValue, |x: RawValueRep| i8::from_ne_bytes(x[0..1].try_into().unwrap()));
impl_from_raw_wasm_value!(i16, |x| x as RawValue, |x: RawValueRep| i16::from_ne_bytes(x[0..2].try_into().unwrap()));
