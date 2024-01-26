#![no_std]
#![no_main]

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

#[link(wasm_import_module = "env")]
extern "C" {
    fn printi32(x: i32);
}

#[no_mangle]
pub unsafe extern "C" fn add_and_print(lh: i32, rh: i32) {
    printi32(lh + rh);
}
