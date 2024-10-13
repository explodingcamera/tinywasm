#![no_main]
#![no_std]
use lol_alloc::{AssumeSingleThreaded, FreeListAllocator};
use tinywasm::{Extern, FuncContext};

extern crate alloc;

#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[global_allocator]
static ALLOCATOR: AssumeSingleThreaded<FreeListAllocator> =
    unsafe { AssumeSingleThreaded::new(FreeListAllocator::new()) };

#[link(wasm_import_module = "env")]
extern "C" {
    fn printi32(x: i32);
}

#[no_mangle]
pub extern "C" fn hello() {
    let _ = run();
}

fn run() -> tinywasm::Result<()> {
    let mut store = tinywasm::Store::default();
    let mut imports = tinywasm::Imports::new();

    let res = tinywasm::parser::Parser::new().parse_module_bytes(include_bytes!("./print.wasm"))?;
    let twasm = res.serialize_twasm();
    let module = tinywasm::Module::parse_bytes(&twasm)?;

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
