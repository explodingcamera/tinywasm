use wasmparser::ValType;

/// A WebAssembly value.
/// See https://webassembly.github.io/spec/core/syntax/types.html#value-types
#[derive(Debug, Clone, PartialEq)]
pub enum WasmValue {
    // Num types
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),

    // Vec types
    V128(i128),
}

impl From<WasmValue> for ValType {
    fn from(wasm_value: WasmValue) -> Self {
        match wasm_value {
            WasmValue::I32(_) => ValType::I32,
            WasmValue::I64(_) => ValType::I64,
            WasmValue::F32(_) => ValType::F32,
            WasmValue::F64(_) => ValType::F64,
            WasmValue::V128(_) => ValType::V128,
        }
    }
}

impl WasmValue {
    pub fn type_of(&self) -> ValType {
        self.clone().into()
    }

    pub fn is(&self, ty: ValType) -> bool {
        self.type_of() == ty
    }
}
