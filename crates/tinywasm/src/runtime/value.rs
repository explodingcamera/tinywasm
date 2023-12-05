use tinywasm_types::{ValType, WasmValue};

#[derive(Debug, Clone, Copy, Default)]
pub struct UntypedWasmValue(u64);

impl UntypedWasmValue {
    pub fn into_typed(self, ty: ValType) -> WasmValue {
        match ty {
            ValType::I32 => WasmValue::I32(self.0 as i32),
            ValType::I64 => WasmValue::I64(self.0 as i64),
            ValType::F32 => WasmValue::F32(f32::from_bits(self.0 as u32)),
            ValType::F64 => WasmValue::F64(f64::from_bits(self.0)),
            ValType::ExternRef => todo!(),
            ValType::FuncRef => todo!(),
            ValType::V128 => todo!(),
        }
    }
}

impl From<i32> for UntypedWasmValue {
    fn from(i: i32) -> Self {
        Self(i as u64)
    }
}

impl From<UntypedWasmValue> for i32 {
    fn from(v: UntypedWasmValue) -> Self {
        v.0 as i32
    }
}

impl From<i64> for UntypedWasmValue {
    fn from(i: i64) -> Self {
        Self(i as u64)
    }
}

impl From<UntypedWasmValue> for i64 {
    fn from(v: UntypedWasmValue) -> Self {
        v.0 as i64
    }
}

impl From<f32> for UntypedWasmValue {
    fn from(i: f32) -> Self {
        Self(i.to_bits() as u64)
    }
}

impl From<UntypedWasmValue> for f32 {
    fn from(v: UntypedWasmValue) -> Self {
        f32::from_bits(v.0 as u32)
    }
}

impl From<f64> for UntypedWasmValue {
    fn from(i: f64) -> Self {
        Self(i.to_bits())
    }
}

impl From<UntypedWasmValue> for f64 {
    fn from(v: UntypedWasmValue) -> Self {
        f64::from_bits(v.0)
    }
}

impl From<WasmValue> for UntypedWasmValue {
    fn from(v: WasmValue) -> Self {
        match v {
            WasmValue::I32(i) => Self(i as u64),
            WasmValue::I64(i) => Self(i as u64),
            WasmValue::F32(i) => Self(i.to_bits() as u64),
            WasmValue::F64(i) => Self(i.to_bits()),
        }
    }
}
