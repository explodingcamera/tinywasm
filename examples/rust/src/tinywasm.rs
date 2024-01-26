#![no_std]
#![no_main]

use embedded_alloc::Heap;
// use tinywasm::{Extern, FuncContext};

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[no_mangle]
pub unsafe extern "C" fn _start() {
    // Initialize the allocator BEFORE you use it
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 1024;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    // now the allocator is ready types like Box, Vec can be used.
    let _ = run();
}

fn run() -> tinywasm::Result<()> {
    // let module = tinywasm::Module::parse_bytes(include_bytes!("../out/hello.wasm"))?;
    // let mut store = tinywasm::Store::default();

    // let mut imports = tinywasm::Imports::new();
    // imports.define("env", "printi32", Extern::typed_func(|_: FuncContext<'_>, _: i32| Ok(())))?;

    // let instance = module.instantiate(&mut store, Some(imports))?;
    // let add_and_print = instance.typed_func::<(i32, i32), ()>(&mut store, "add_and_print")?;
    // add_and_print.call(&mut store, (1, 2))?;
    Ok(())
}
