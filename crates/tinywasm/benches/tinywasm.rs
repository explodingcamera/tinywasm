use criterion::{criterion_group, criterion_main, Criterion};
use eyre::Result;
use tinywasm::{types, Extern, FuncContext, Imports, ModuleInstance, Store};
use types::{archive::AlignedVec, TinyWasmModule};

const WASM: &[u8] = include_bytes!("../../../examples/rust/out/tinywasm.opt.wasm");

fn tinywasm_parse() -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    let data = parser.parse_module_bytes(WASM)?;
    Ok(data)
}

fn tinywasm_to_twasm(module: TinyWasmModule) -> Result<AlignedVec> {
    let twasm = module.serialize_twasm();
    Ok(twasm)
}

fn tinywasm_from_twasm(twasm: AlignedVec) -> Result<TinyWasmModule> {
    let module = TinyWasmModule::from_twasm(&twasm)?;
    Ok(module)
}

fn tinywasm_run(module: TinyWasmModule) -> Result<()> {
    let mut store = Store::default();
    let mut imports = Imports::default();
    imports.define("env", "printi32", Extern::typed_func(|_: FuncContext<'_>, _: i32| Ok(()))).expect("define");
    let instance = ModuleInstance::instantiate(&mut store, module.into(), Some(imports)).expect("instantiate");
    let hello = instance.exported_func::<(), ()>(&store, "hello").expect("exported_func");
    hello.call(&mut store, ()).expect("call");
    Ok(())
}

fn criterion_benchmark(c: &mut Criterion) {
    let module = tinywasm_parse().expect("tinywasm_parse");
    let twasm = tinywasm_to_twasm(module.clone()).expect("tinywasm_to_twasm");

    c.bench_function("tinywasm_parse", |b| b.iter(tinywasm_parse));
    c.bench_function("tinywasm_to_twasm", |b| b.iter(|| tinywasm_to_twasm(module.clone())));
    c.bench_function("tinywasm_from_twasm", |b| b.iter(|| tinywasm_from_twasm(twasm.clone())));
    c.bench_function("tinywasm", |b| b.iter(|| tinywasm_run(module.clone())));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
