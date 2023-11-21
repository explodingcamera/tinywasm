use wasmcore::{self, Module};

pub static WASM: &'static [u8] = include_bytes!("../helloworld.wasm");

fn main() {
    let module = Module::new(WASM);

    println!("{:#?}", module);
}
