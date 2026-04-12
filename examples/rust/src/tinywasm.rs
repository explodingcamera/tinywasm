#![no_main]
use tinywasm::{FuncContext, HostFunction};

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn printi32(x: i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn hello() {
    let _ = run();
}

fn run() -> tinywasm::Result<()> {
    let module = tinywasm::Module::parse_stream(&include_bytes!("./print.wasm")[..])?;
    let mut store = tinywasm::Store::default();

    let printi32 = HostFunction::from(&mut store, |_: FuncContext<'_>, v: i32| {
        unsafe { printi32(v) }
        Ok(())
    });

    let mut imports = tinywasm::Imports::new();
    imports.define("env", "printi32", printi32);

    let instance = module.instantiate(&mut store, Some(imports))?;
    let add_and_print = instance.func::<(i32, i32), ()>(&store, "add_and_print")?;
    add_and_print.call(&mut store, (1, 2))?;
    Ok(())
}
