#![no_main]

use tinywasm::{ModuleInstance, Store};

const INNER: &[u8] = include_bytes!("inner.wasm");

#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    let module = tinywasm::parse_bytes(INNER).unwrap();
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();
    let add = instance.func::<(i32, i32), i32>(&store, "add").unwrap();
    (0..1000).map(|value| add.call(&mut store, (value, 1)).unwrap()).sum()
}
