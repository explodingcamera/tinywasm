use std::hint::black_box;

use eyre::{eyre, Result};
use tinywasm::{Extern, FuncContext, Imports, MemoryStringExt, Module, Store};

/// Examples of using WebAssembly compiled from Rust with tinywasm.
///
/// These examples are meant to be run with `cargo run --example wasm-rust <example>`.
/// For example, `cargo run --example wasm-rust hello`.
///
/// To run these, you first need to compile the Rust examples to WebAssembly:
///
/// ```sh
/// ./examples/rust/build.sh
/// ```
///
/// This requires the `wasm32-unknown-unknown` target, `binaryen` and `wabt` to be installed.
/// `rustup target add wasm32-unknown-unknown`.
/// <https://github.com/WebAssembly/wabt>
/// <https://github.com/WebAssembly/binaryen>
///
fn main() -> Result<()> {
    pretty_env_logger::init();

    if !std::path::Path::new("./examples/rust/out/").exists() {
        return Err(eyre!("No WebAssembly files found. See examples/wasm-rust.rs for instructions."));
    }

    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 {
        println!("Usage: cargo run --example wasm-rust <rust_example>");
        println!("Available examples:");
        println!("  hello");
        println!("  printi32");
        println!("  fibonacci - calculate fibonacci(30)");
        println!("  tinywasm - run printi32 inside of tinywasm inside of itself");
        println!("  argon2id - run argon2id(1000, 2, 1)");
        return Ok(());
    }

    match args[1].as_str() {
        "hello" => hello()?,
        "printi32" => printi32()?,
        "fibonacci" => fibonacci()?,
        "tinywasm" => tinywasm()?,
        "tinywasm_no_std" => tinywasm_no_std()?,
        "argon2id" => argon2id()?,
        "all" => {
            println!("Running all examples");
            println!("\nhello.wasm:");
            hello()?;
            println!("\nprinti32.wasm:");
            printi32()?;
            println!("\nfibonacci.wasm:");
            fibonacci()?;
            println!("\ntinywasm.wasm:");
            tinywasm()?;
            println!("\ntinywasm_no_std.wasm:");
            tinywasm_no_std()?;
            println!("argon2id.wasm:");
            argon2id()?;
        }
        _ => {}
    }

    Ok(())
}

fn tinywasm() -> Result<()> {
    let module = Module::parse_file("./examples/rust/out/tinywasm.opt.wasm")?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define("env", "printi32", Extern::typed_func(|_: FuncContext<'_>, _x: i32| Ok(())))?;
    let instance = module.instantiate(&mut store, Some(black_box(imports)))?;

    let hello = instance.exported_func::<(), ()>(&store, "hello")?;
    hello.call(&mut store, black_box(()))?;
    hello.call(&mut store, black_box(()))?;
    hello.call(&mut store, black_box(()))?;
    Ok(())
}

fn tinywasm_no_std() -> Result<()> {
    let module = Module::parse_file("./examples/rust/out/tinywasm_no_std.opt.wasm")?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define("env", "printi32", Extern::typed_func(|_: FuncContext<'_>, _x: i32| Ok(())))?;
    let instance = module.instantiate(&mut store, Some(black_box(imports)))?;

    let hello = instance.exported_func::<(), ()>(&store, "hello")?;
    hello.call(&mut store, black_box(()))?;
    hello.call(&mut store, black_box(()))?;
    hello.call(&mut store, black_box(()))?;
    Ok(())
}

fn hello() -> Result<()> {
    let module = Module::parse_file("./examples/rust/out/hello.opt.wasm")?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define(
        "env",
        "print_utf8",
        Extern::typed_func(|mut ctx: FuncContext<'_>, args: (i64, i32)| {
            let mem = ctx.exported_memory("memory")?;
            let ptr = args.0 as usize;
            let len = args.1 as usize;
            let string = mem.load_string(ptr, len)?;
            println!("{string}");
            Ok(())
        }),
    )?;

    let instance = module.instantiate(&mut store, Some(imports))?;
    let arg_ptr = instance.exported_func::<(), i32>(&store, "arg_ptr")?.call(&mut store, ())?;
    let arg = b"world";

    instance.exported_memory_mut(&mut store, "memory")?.store(arg_ptr as usize, arg.len(), arg)?;
    let hello = instance.exported_func::<i32, ()>(&store, "hello")?;
    hello.call(&mut store, arg.len() as i32)?;

    Ok(())
}

fn printi32() -> Result<()> {
    let module = Module::parse_file("./examples/rust/out/print.opt.wasm")?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define(
        "env",
        "printi32",
        Extern::typed_func(|_: FuncContext<'_>, x: i32| {
            println!("{x}");
            Ok(())
        }),
    )?;

    let instance = module.instantiate(&mut store, Some(imports))?;
    let add_and_print = instance.exported_func::<(i32, i32), ()>(&store, "add_and_print")?;
    add_and_print.call(&mut store, (1, 2))?;

    Ok(())
}

fn fibonacci() -> Result<()> {
    let module = Module::parse_file("./examples/rust/out/fibonacci.opt.wasm")?;
    let mut store = Store::default();

    let instance = module.instantiate(&mut store, None)?;
    let fibonacci = instance.exported_func::<i32, i32>(&store, "fibonacci_recursive")?;
    let n = 26;
    let result = fibonacci.call(&mut store, n)?;
    println!("fibonacci({n}) = {result}");

    Ok(())
}

fn argon2id() -> Result<()> {
    let module = Module::parse_file("./examples/rust/out/argon2id.opt.wasm")?;
    let mut store = Store::default();

    let instance = module.instantiate(&mut store, None)?;
    let argon2id = instance.exported_func::<(i32, i32, i32), i32>(&store, "argon2id")?;
    argon2id.call(&mut store, (1000, 2, 1))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello() {
        hello().unwrap();
    }

    #[test]
    fn test_printi32() {
        printi32().unwrap();
    }

    #[test]
    fn test_fibonacci() {
        fibonacci().unwrap();
    }

    #[test]
    fn test_tinywasm() {
        tinywasm().unwrap();
    }

    #[test]
    fn test_tinywasm_no_std() {
        tinywasm_no_std().unwrap();
    }

    #[test]
    fn test_argon2id() {
        argon2id().unwrap();
    }
}
