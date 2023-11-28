// https://webassembly.github.io/spec/core/binary/instructions.html

// Controll Instructions
pub mod control {
    pub const WASM_UNREACHABLE: u8 = 0x00;
    pub const WASM_NOP: u8 = 0x01; // do nothing

    // stuctured instructions
    pub const WASM_BLOCK: u8 = 0x02;
    pub const WASM_LOOP: u8 = 0x03;
    pub const WASM_IF: u8 = 0x04;

    pub const WASM_ELSE: u8 = 0x05;
    pub const WASM_END: u8 = 0x0B;
    pub const WASM_BR: u8 = 0x0C;
    pub const WASM_BR_IF: u8 = 0x0D;
    pub const WASM_BR_TABLE: u8 = 0x0E;
    pub const WASM_RETURN: u8 = 0x0F;
    pub const WASM_CALL: u8 = 0x10;
    pub const WASM_CALL_INDIRECT: u8 = 0x11;
    pub const WASM_DROP: u8 = 0x1A;
}

// Reference Instructions
pub mod reference {
    pub const WASM_REF_NULL: u8 = 0xD0;
    pub const WASM_REF_IS_NULL: u8 = 0xD1;
    pub const WASM_REF_FUNC: u8 = 0xD2;
}

// Parametric Instructions
pub mod parametric {
    pub const WASM_DROP: u8 = 0x1A;
    pub const WASM_SELECT: u8 = 0x1B;
    pub const WASM_SELECT_T: u8 = 0x1C;
}

// Variable Instructions
pub mod variable {
    pub const WASM_LOCAL_GET: u8 = 0x20;
    pub const WASM_LOCAL_SET: u8 = 0x21;
    pub const WASM_LOCAL_TEE: u8 = 0x22;
    pub const WASM_GLOBAL_GET: u8 = 0x23;
    pub const WASM_GLOBAL_SET: u8 = 0x24;
}

// Table Instructions
pub mod table {
    pub const WASM_TABLE_GET: u8 = 0x25;
    pub const WASM_TABLE_SET: u8 = 0x26;
    pub const WASM_TABLE_INIT: u8 = 0xFC;
    pub const WASM_ELEM_DROP: u8 = 0xFC;
    pub const WASM_TABLE_COPY: u8 = 0xFC;
    pub const WASM_TABLE_GROW: u8 = 0xFC;
    pub const WASM_TABLE_SIZE: u8 = 0xFC;
    pub const WASM_TABLE_FILL: u8 = 0xFC;
}

// Memory Instructions
pub mod memory {
    pub const WASM_I32_LOAD: u8 = 0x28;
    pub const WASM_I64_LOAD: u8 = 0x29;
    pub const WASM_F32_LOAD: u8 = 0x2A;
    pub const WASM_F64_LOAD: u8 = 0x2B;
    pub const WASM_I32_LOAD8_S: u8 = 0x2C;
    pub const WASM_I32_LOAD8_U: u8 = 0x2D;
    pub const WASM_I32_LOAD16_S: u8 = 0x2E;
    pub const WASM_I32_LOAD16_U: u8 = 0x2F;
    pub const WASM_I64_LOAD8_S: u8 = 0x30;
    pub const WASM_I64_LOAD8_U: u8 = 0x31;
    pub const WASM_I64_LOAD16_S: u8 = 0x32;
    pub const WASM_I64_LOAD16_U: u8 = 0x33;
    pub const WASM_I64_LOAD32_S: u8 = 0x34;
    pub const WASM_I64_LOAD32_U: u8 = 0x35;
    pub const WASM_I32_STORE: u8 = 0x36;
    pub const WASM_I64_STORE: u8 = 0x37;
    pub const WASM_F32_STORE: u8 = 0x38;
    pub const WASM_F64_STORE: u8 = 0x39;
    pub const WASM_I32_STORE8: u8 = 0x3A;
    pub const WASM_I32_STORE16: u8 = 0x3B;
    pub const WASM_I64_STORE8: u8 = 0x3C;
    pub const WASM_I64_STORE16: u8 = 0x3D;
    pub const WASM_I64_STORE32: u8 = 0x3E;
    pub const WASM_MEMORY_SIZE: u8 = 0x3F;
    pub const WASM_MEMORY_GROW: u8 = 0x40;
    pub const WASM_MEMORY_INIT: u8 = 0xFC;
    pub const WASM_DATA_DROP: u8 = 0xFC;
    pub const WASM_MEMORY_COPY: u8 = 0xFC;
    pub const WASM_MEMORY_FILL: u8 = 0xFC;
}

// Numeric Instructions
pub mod numeric {
    // Constants
    pub const WASM_I32_CONST: u8 = 0x41;
    pub const WASM_I64_CONST: u8 = 0x42;
    pub const WASM_F32_CONST: u8 = 0x43;
    pub const WASM_F64_CONST: u8 = 0x44;

    // Operations
    pub const START_NUMERIC: u8 = 0x45;
    pub const END_NUMERIC: u8 = 0xC4;
    pub const WASM_SATURATING_TRUNC: u8 = 0xFC;
}

pub const WASM_VEC: u8 = 0xFD;
