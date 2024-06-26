#![allow(missing_docs)]
use tinywasm_types::{ValType, WasmValue};

pub type Value32 = u32;
pub type Value64 = u64;
pub type Value128 = u128;
pub type ValueRef = Option<u32>;

pub const VALUE32: u8 = 0;
pub const VALUE64: u8 = 1;
pub const VALUE128: u8 = 2;
pub const VALUEREF: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TinyWasmValue {
    Value32(Value32),
    Value64(Value64),
    Value128(Value128),
    ValueRef(ValueRef),
}

impl TinyWasmValue {
    pub fn unwrap_32(&self) -> Value32 {
        match self {
            TinyWasmValue::Value32(v) => *v,
            _ => unreachable!("Expected Value32"),
        }
    }

    pub fn unwrap_64(&self) -> Value64 {
        match self {
            TinyWasmValue::Value64(v) => *v,
            _ => unreachable!("Expected Value64"),
        }
    }

    pub fn unwrap_128(&self) -> Value128 {
        match self {
            TinyWasmValue::Value128(v) => *v,
            _ => unreachable!("Expected Value128"),
        }
    }

    pub fn unwrap_ref(&self) -> ValueRef {
        match self {
            TinyWasmValue::ValueRef(v) => *v,
            _ => unreachable!("Expected ValueRef"),
        }
    }

    pub fn attach_type(&self, ty: ValType) -> WasmValue {
        match ty {
            ValType::I32 => WasmValue::I32(self.unwrap_32() as i32),
            ValType::I64 => WasmValue::I64(self.unwrap_64() as i64),
            ValType::F32 => WasmValue::F32(f32::from_bits(self.unwrap_32())),
            ValType::F64 => WasmValue::F64(f64::from_bits(self.unwrap_64())),
            ValType::V128 => WasmValue::V128(self.unwrap_128()),
            ValType::RefExtern => match self.unwrap_ref() {
                Some(v) => WasmValue::RefExtern(v),
                None => WasmValue::RefNull(ValType::RefExtern),
            },
            ValType::RefFunc => match self.unwrap_ref() {
                Some(v) => WasmValue::RefFunc(v),
                None => WasmValue::RefNull(ValType::RefFunc),
            },
        }
    }
}

impl Default for TinyWasmValue {
    fn default() -> Self {
        TinyWasmValue::Value32(0)
    }
}

impl From<WasmValue> for TinyWasmValue {
    fn from(value: WasmValue) -> Self {
        match value {
            WasmValue::I32(v) => TinyWasmValue::Value32(v as u32),
            WasmValue::I64(v) => TinyWasmValue::Value64(v as u64),
            WasmValue::V128(v) => TinyWasmValue::Value128(v),
            WasmValue::F32(v) => TinyWasmValue::Value32(v.to_bits()),
            WasmValue::F64(v) => TinyWasmValue::Value64(v.to_bits()),
            WasmValue::RefFunc(v) => TinyWasmValue::ValueRef(Some(v)),
            WasmValue::RefExtern(v) => TinyWasmValue::ValueRef(Some(v)),
            WasmValue::RefNull(_) => TinyWasmValue::ValueRef(None),
        }
    }
}

impl From<&WasmValue> for TinyWasmValue {
    fn from(value: &WasmValue) -> Self {
        match value {
            WasmValue::I32(v) => TinyWasmValue::Value32(*v as u32),
            WasmValue::I64(v) => TinyWasmValue::Value64(*v as u64),
            WasmValue::V128(v) => TinyWasmValue::Value128(*v),
            WasmValue::F32(v) => TinyWasmValue::Value32(v.to_bits()),
            WasmValue::F64(v) => TinyWasmValue::Value64(v.to_bits()),
            WasmValue::RefFunc(v) => TinyWasmValue::ValueRef(Some(*v)),
            WasmValue::RefExtern(v) => TinyWasmValue::ValueRef(Some(*v)),
            WasmValue::RefNull(_) => TinyWasmValue::ValueRef(None),
        }
    }
}

impl From<f32> for TinyWasmValue {
    fn from(value: f32) -> Self {
        TinyWasmValue::Value32(value.to_bits())
    }
}

impl From<f64> for TinyWasmValue {
    fn from(value: f64) -> Self {
        TinyWasmValue::Value64(value.to_bits())
    }
}

impl From<i32> for TinyWasmValue {
    fn from(value: i32) -> Self {
        TinyWasmValue::Value32(value as u32)
    }
}

impl From<i64> for TinyWasmValue {
    fn from(value: i64) -> Self {
        TinyWasmValue::Value64(value as u64)
    }
}
