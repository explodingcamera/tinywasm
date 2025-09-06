#![no_main]

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn bar(left: i64, right: i32) -> i32;
}

#[unsafe(no_mangle)]
pub fn foo() -> i32 {
    unsafe { bar(1, 2) }
}
