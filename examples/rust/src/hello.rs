#![no_main]

#[link(wasm_import_module = "env")]
extern "C" {
    fn print_utf8(location: i64, len: i32);
}

const ARG: &[u8] = &[0u8; 100];

#[no_mangle]
pub unsafe extern "C" fn arg_ptr() -> i32 {
    ARG.as_ptr() as i32
}

#[no_mangle]
pub unsafe extern "C" fn arg_size() -> i32 {
    ARG.len() as i32
}

#[no_mangle]
pub unsafe extern "C" fn hello(len: i32) {
    let arg = core::str::from_utf8(&ARG[0..len as usize]).unwrap();
    let res = format!("Hello, {}!", arg).as_bytes().to_vec();

    let len = res.len() as i32;
    let ptr = res.leak().as_ptr() as i64;
    print_utf8(ptr, len);
}
