use color_eyre::eyre::Result;
use tinywasm::{Extern, FuncContext, Imports, Module, Store};

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 {
        println!("Usage: cargo run --example wasm-rust <rust_example>");
        println!("Available examples:");
        println!("  hello");
        println!("  tinywasm");
        return Ok(());
    }

    match args[1].as_str() {
        "hello" => hello()?,
        "tinywasm" => tinywasm()?,
        _ => {}
    }

    Ok(())
}

fn tinywasm() -> Result<()> {
    const TINYWASM: &[u8] = include_bytes!("./rust/out/tinywasm.wasm");
    let module = Module::parse_bytes(&TINYWASM)?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define(
        "env",
        "printi32",
        Extern::typed_func(|_: FuncContext<'_>, x: i32| {
            println!("{}", x);
            Ok(())
        }),
    )?;
    let instance = module.instantiate(&mut store, Some(imports))?;

    let hello = instance.typed_func::<(), ()>(&mut store, "hello")?;
    hello.call(&mut store, ())?;

    Ok(())
}

fn hello() -> Result<()> {
    const HELLO_WASM: &[u8] = include_bytes!("./rust/out/hello.wasm");
    let module = Module::parse_bytes(&HELLO_WASM)?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define(
        "env",
        "printi32",
        Extern::typed_func(|_: FuncContext<'_>, x: i32| {
            println!("{}", x);
            Ok(())
        }),
    )?;

    let instance = module.instantiate(&mut store, Some(imports))?;
    let add_and_print = instance.typed_func::<(i32, i32), ()>(&mut store, "add_and_print")?;
    add_and_print.call(&mut store, (1, 2))?;

    Ok(())
}
