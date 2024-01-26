#![no_main]
use tinywasm::{Extern, FuncContext};

#[link(wasm_import_module = "env")]
extern "C" {
    fn printi32(x: i32);
}

#[no_mangle]
pub extern "C" fn hello() {
    let _ = run();
}

fn run() -> tinywasm::Result<()> {
    let module = tinywasm::Module::parse_bytes(include_bytes!("../out/print.wasm"))?;
    let mut store = tinywasm::Store::default();
    let mut imports = tinywasm::Imports::new();

    imports.define(
        "env",
        "printi32",
        Extern::typed_func(|_: FuncContext<'_>, v: i32| {
            unsafe { printi32(v) }
            Ok(())
        }),
    )?;
    let instance = module.instantiate(&mut store, Some(imports))?;

    let add_and_print = instance.exported_func::<(i32, i32), ()>(&mut store, "add_and_print")?;
    add_and_print.call(&mut store, (1, 2))?;
    Ok(())
}
