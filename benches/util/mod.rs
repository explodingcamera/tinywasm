use tinywasm::{self, parser::Parser, types::TinyWasmModule};

pub fn tinywasm_module(wasm: &[u8]) -> TinyWasmModule {
    let parser = Parser::new();
    parser.parse_module_bytes(wasm).expect("parse_module_bytes")
}
