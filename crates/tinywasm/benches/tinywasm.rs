use criterion::{Criterion, criterion_group, criterion_main};
use eyre::Result;
use tinywasm::{
    Engine, FuncContext, HostFunction, Imports, MemoryBackend, ModuleInstance, Store, engine::Config, types,
};
use types::Module;

const WASM: &[u8] = include_bytes!("../../../examples/rust/out/tinywasm.wasm");

fn tinywasm_parse() -> Result<Module> {
    let parser = tinywasm_parser::Parser::new();
    let data = parser.parse_module_bytes(WASM)?;
    Ok(data)
}

fn tinywasm_to_twasm(module: &Module) -> Result<Vec<u8>> {
    let twasm = module.serialize_twasm()?;
    Ok(twasm)
}

fn tinywasm_from_twasm(twasm: &[u8]) -> Result<Module> {
    let module = Module::try_from_twasm(twasm)?;
    Ok(module)
}

fn tinywasm_run(module: &Module) -> Result<()> {
    let engine = Engine::new(Config::default().with_memory_backend(MemoryBackend::paged(64 * 1024)));
    let mut store = Store::new(engine);
    let mut imports = Imports::default();
    imports.define("env", "printi32", HostFunction::from(&mut store, |_: FuncContext<'_>, _: i32| Ok(())));
    let instance = ModuleInstance::instantiate(&mut store, module, Some(imports)).expect("instantiate");
    let hello = instance.func::<(), ()>(&store, "hello").expect("func_typed");
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
    group.bench_function("tinywasm", |b| b.iter(|| tinywasm_run(&module)));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
