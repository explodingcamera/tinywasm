use core::fmt::Debug;
use tinywasm_types::{ValType, WasmValue};

/// A raw wasm value.
///
/// This is the internal representation of all wasm values
///
/// See [`WasmValue`] for the public representation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
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
            ValType::RefExtern => match i64::from(self) {
                v if v < 0 => WasmValue::RefNull(ValType::RefExtern),
                addr => WasmValue::RefExtern(addr as u32),
            },
            ValType::RefFunc => match i64::from(self) {
                v if v < 0 => WasmValue::RefNull(ValType::RefFunc),
                addr => WasmValue::RefFunc(addr as u32),
            },
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

// This all looks like a lot of extra steps, but the compiler will optimize it all away.
impl_from_raw_wasm_value!(i32, |x| x as u64, |x: [u8; 8]| i32::from_ne_bytes(x[0..4].try_into().unwrap()));
impl_from_raw_wasm_value!(i64, |x| x as u64, |x: [u8; 8]| i64::from_ne_bytes(x[0..8].try_into().unwrap()));
impl_from_raw_wasm_value!(u8, |x| x as u64, |x: [u8; 8]| u8::from_ne_bytes(x[0..1].try_into().unwrap()));
impl_from_raw_wasm_value!(u16, |x| x as u64, |x: [u8; 8]| u16::from_ne_bytes(x[0..2].try_into().unwrap()));
impl_from_raw_wasm_value!(u32, |x| x as u64, |x: [u8; 8]| u32::from_ne_bytes(x[0..4].try_into().unwrap()));
impl_from_raw_wasm_value!(u64, |x| x, |x: [u8; 8]| u64::from_ne_bytes(x[0..8].try_into().unwrap()));
impl_from_raw_wasm_value!(i8, |x| x as u64, |x: [u8; 8]| i8::from_ne_bytes(x[0..1].try_into().unwrap()));
impl_from_raw_wasm_value!(i16, |x| x as u64, |x: [u8; 8]| i16::from_ne_bytes(x[0..2].try_into().unwrap()));
impl_from_raw_wasm_value!(f32, |x| f32::to_bits(x) as u64, |x: [u8; 8]| f32::from_ne_bytes(x[0..4].try_into().unwrap()));
impl_from_raw_wasm_value!(f64, f64::to_bits, |x: [u8; 8]| f64::from_bits(u64::from_ne_bytes(x[0..8].try_into().unwrap())));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_wasm_value() {
        macro_rules! test_macro {
            ($( $ty:ty => $val:expr ),*) => {
            $(
                let raw: RawWasmValue = $val.into();
                let val: $ty = raw.into();
                assert_eq!(val, $val);
            )*
            };
        }

        test_macro! {
             i32 => 0, i64 => 0, u8 => 0, u16 => 0, u32 => 0, u64 => 0, i8 => 0, i16 => 0, f32 => 0.0, f64 => 0.0,
             i32 => i32::MIN, i64 => i64::MIN, u8 => u8::MIN, u16 => u16::MIN, u32 => u32::MIN, u64 => u64::MIN, i8 => i8::MIN, i16 => i16::MIN, f32 => f32::MIN, f64 => f64::MIN,
             i32 => i32::MAX, i64 => i64::MAX, u8 => u8::MAX, u16 => u16::MAX, u32 => u32::MAX, u64 => u64::MAX, i8 => i8::MAX, i16 => i16::MAX, f32 => f32::MAX, f64 => f64::MAX
        }
    }
}
