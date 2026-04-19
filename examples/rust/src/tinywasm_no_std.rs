#![no_main]
#![no_std]
use dlmalloc::GlobalDlmalloc;
use tinywasm::{FuncContext, HostFunction, ModuleInstance};

extern crate alloc;

#[global_allocator]
static ALLOCATOR: GlobalDlmalloc = GlobalDlmalloc;

#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn printi32(x: i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn hello() {
    let _ = run();
}

fn run() -> tinywasm::Result<()> {
    let mut store = tinywasm::Store::default();
    let mut imports = tinywasm::Imports::new();

    let res = tinywasm::parser::Parser::new().parse_module_bytes(include_bytes!("./print.wasm"))?;
    let twasm = res.serialize_twasm()?;
    let module = tinywasm::parse_bytes(&twasm)?;

    let printi32 = HostFunction::from(&mut store, |_: FuncContext<'_>, v: i32| {
        unsafe { printi32(v) }
        Ok(())
    });

    imports.define("env", "printi32", printi32);
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
    let add_and_print = instance.func::<(i32, i32), ()>(&store, "add_and_print")?;
    add_and_print.call(&mut store, (1, 2))?;
    Ok(())
}
