#![allow(dead_code)]

use tinywasm::{self, parser::Parser, types::TinyWasmModule};

pub fn parse_wasm(wasm: &[u8]) -> TinyWasmModule {
    let parser = Parser::new();
    parser.parse_module_bytes(wasm).expect("parse_module_bytes")
}

pub fn wasm_to_twasm(wasm: &[u8]) -> Vec<u8> {
    let parser = Parser::new();
    let res = parser.parse_module_bytes(wasm).expect("parse_module_bytes");
    res.serialize_twasm().to_vec()
}

#[inline]
pub fn twasm_to_module(twasm: &[u8]) -> tinywasm::Module {
    unsafe { TinyWasmModule::from_twasm_unchecked(twasm) }.into()
}

pub fn tinywasm(twasm: &[u8]) -> (tinywasm::Store, tinywasm::ModuleInstance) {
    use tinywasm::*;
    let module = twasm_to_module(twasm);
    let mut store = Store::default();
    let imports = Imports::default();
    let instance = ModuleInstance::instantiate(&mut store, module, Some(imports)).expect("instantiate");
    (store, instance)
}

pub fn wasmi(wasm: &[u8]) -> (wasmi::Module, wasmi::Store<()>, wasmi::Linker<()>) {
    use wasmi::*;
    let engine = Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("wasmi::Module::new");
    let store = Store::new(&engine, ());
    let linker = <Linker<()>>::new(&engine);
    (module, store, linker)
}

pub fn wasmer(wasm: &[u8]) -> (wasmer::Store, wasmer::Instance) {
    use wasmer::*;
    let compiler = Singlepass::default();
    let mut store = Store::new(compiler);
    let import_object = imports! {};
    let module = Module::new(&store, wasm).expect("wasmer::Module::new");
    let instance = Instance::new(&mut store, &module, &import_object).expect("Instance::new");
    (store, instance)
}
