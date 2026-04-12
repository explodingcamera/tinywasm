use criterion::{Criterion, criterion_group, criterion_main};
use eyre::Result;
use tinywasm::{ModuleInstance, Store, types};
use types::TinyWasmModule;

const WASM: &[u8] = include_bytes!("../../../examples/rust/out/argon2id.wasm");

fn argon2id_parse() -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    let data = parser.parse_module_bytes(WASM)?;
    Ok(data)
}

fn argon2id_to_twasm(module: &TinyWasmModule) -> Result<Vec<u8>> {
    let twasm = module.serialize_twasm()?;
    Ok(twasm)
}

fn argon2id_from_twasm(twasm: &[u8]) -> Result<TinyWasmModule> {
    let module = TinyWasmModule::from_twasm(twasm)?;
    Ok(module)
}

fn argon2id_run(module: TinyWasmModule) -> Result<()> {
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, module.into(), None)?;
    let argon2 = instance.func::<(i32, i32, i32), i32>(&store, "argon2id")?;
    argon2.call(&mut store, (1000, 2, 1))?;
    Ok(())
}

fn criterion_benchmark(c: &mut Criterion) {
    let module = argon2id_parse().expect("argon2id_parse");
    let twasm = argon2id_to_twasm(&module).expect("argon2id_to_twasm");
    let mut group = c.benchmark_group("argon2id");

    group.measurement_time(std::time::Duration::from_secs(2));
    group.bench_function("argon2id_parse", |b| b.iter(argon2id_parse));
    group.bench_function("argon2id_to_twasm", |b| b.iter(|| argon2id_to_twasm(&module)));
    group.bench_function("argon2id_from_twasm", |b| b.iter(|| argon2id_from_twasm(&twasm)));
    group.measurement_time(std::time::Duration::from_secs(10));
    group.bench_function("argon2id", |b| b.iter(|| argon2id_run(module.clone())));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
