#![allow(dead_code)]

pub fn tinywasm(wasm: &[u8]) -> (tinywasm::Store, tinywasm::ModuleInstance) {
    use tinywasm::*;
    let module = Module::parse_bytes(wasm).expect("Module::parse_bytes");
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
