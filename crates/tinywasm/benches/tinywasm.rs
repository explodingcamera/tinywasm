use criterion::{Criterion, criterion_group, criterion_main};
use eyre::Result;
use tinywasm::{Extern, FuncContext, Imports, ModuleInstance, Store, types};
use types::TinyWasmModule;

const WASM: &[u8] = include_bytes!("../../../examples/rust/out/tinywasm.wasm");

fn tinywasm_parse() -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    let data = parser.parse_module_bytes(WASM)?;
    Ok(data)
}

fn tinywasm_to_twasm(module: &TinyWasmModule) -> Result<Vec<u8>> {
    let twasm = module.serialize_twasm()?;
    Ok(twasm)
}

fn tinywasm_from_twasm(twasm: &[u8]) -> Result<TinyWasmModule> {
    let module = TinyWasmModule::from_twasm(twasm)?;
    Ok(module)
}

fn tinywasm_run(module: TinyWasmModule) -> Result<()> {
    let mut store = Store::default();
    let mut imports = Imports::default();
    imports.define("env", "printi32", Extern::typed_func(|_: FuncContext<'_>, _: i32| Ok(()))).expect("define");
    let instance = ModuleInstance::instantiate(&mut store, module.into(), Some(imports)).expect("instantiate");
    let hello = instance.func_typed::<(), ()>(&store, "hello").expect("func_typed");
    hello.call(&mut store, ()).expect("call");
    Ok(())
}

fn criterion_benchmark(c: &mut Criterion) {
    let module = tinywasm_parse().expect("tinywasm_parse");
    let twasm = tinywasm_to_twasm(&module).expect("tinywasm_to_twasm");
    let mut group = c.benchmark_group("tinywasm");

    group.measurement_time(std::time::Duration::from_secs(2));
    group.bench_function("tinywasm_parse", |b| b.iter(tinywasm_parse));
    group.bench_function("tinywasm_to_twasm", |b| b.iter(|| tinywasm_to_twasm(&module)));
    group.bench_function("tinywasm_from_twasm", |b| b.iter(|| tinywasm_from_twasm(&twasm)));
    group.measurement_time(std::time::Duration::from_secs(10));
    group.bench_function("tinywasm", |b| b.iter(|| tinywasm_run(module.clone())));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
