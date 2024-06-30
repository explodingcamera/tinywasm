use criterion::{criterion_group, criterion_main, Criterion};
use eyre::Result;
use tinywasm::*;
use types::{archive::AlignedVec, TinyWasmModule};

const WASM: &[u8] = include_bytes!("../../../examples/rust/out/fibonacci.opt.wasm");

fn fibonacci_parse() -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    let data = parser.parse_module_bytes(WASM)?;
    Ok(data)
}

fn fibonacci_to_twasm(module: TinyWasmModule) -> Result<AlignedVec> {
    let twasm = module.serialize_twasm();
    Ok(twasm)
}

fn fibonacci_from_twasm(twasm: AlignedVec) -> Result<TinyWasmModule> {
    let module = TinyWasmModule::from_twasm(&twasm)?;
    Ok(module)
}

fn fibonacci_run(module: TinyWasmModule, recursive: bool, n: i32) -> Result<()> {
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, module.into(), None)?;
    let argon2 = instance.exported_func::<i32, i32>(
        &store,
        match recursive {
            true => "fibonacci_recursive",
            false => "fibonacci",
        },
    )?;
    argon2.call(&mut store, n)?;
    Ok(())
}

fn criterion_benchmark(c: &mut Criterion) {
    let module = fibonacci_parse().expect("fibonacci_parse");
    let twasm = fibonacci_to_twasm(module.clone()).expect("fibonacci_to_twasm");

    c.bench_function("fibonacci_parse", |b| b.iter(fibonacci_parse));
    c.bench_function("fibonacci_to_twasm", |b| b.iter(|| fibonacci_to_twasm(module.clone())));
    c.bench_function("fibonacci_from_twasm", |b| b.iter(|| fibonacci_from_twasm(twasm.clone())));
    c.bench_function("fibonacci_iterative_60", |b| b.iter(|| fibonacci_run(module.clone(), false, 60)));
    c.bench_function("fibonacci_recursive_26", |b| b.iter(|| fibonacci_run(module.clone(), true, 26)));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
