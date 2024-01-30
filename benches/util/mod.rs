#![allow(dead_code)]

use tinywasm::{self, parser::Parser, types::TinyWasmModule};

pub fn wasm_to_twasm(wasm: &[u8]) -> Vec<u8> {
    let parser = Parser::new();
    let res = parser.parse_module_bytes(wasm).expect("parse_module_bytes");
    res.serialize_twasm().to_vec()
}

#[inline]
pub fn twasm_to_module(twasm: &[u8]) -> tinywasm::Module {
    unsafe { TinyWasmModule::from_twasm_unchecked(&twasm) }.into()
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
    let engine: Engine = wasmer::Singlepass::default().into();
    let mut store = Store::default();
    let import_object = imports! {};
    let module = wasmer::Module::from_binary(&engine, &wasm).expect("wasmer::Module::from_binary");
    let instance = Instance::new(&mut store, &module, &import_object).expect("Instance::new");
    (store, instance)
}
