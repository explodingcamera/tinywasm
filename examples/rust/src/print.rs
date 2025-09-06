#![no_main]

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn printi32(x: i32);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn add_and_print(lh: i32, rh: i32) {
    printi32(lh + rh);
}
