mod util;
use criterion::{criterion_group, criterion_main, Criterion};
use tinywasm::types::TinyWasmModule;
use util::tinywasm_module;

fn run_tinywasm(module: TinyWasmModule) {
    use tinywasm::*;
    let module = Module::from(module);
    let mut store = Store::default();
    let imports = Imports::default();
    let instance = ModuleInstance::instantiate(&mut store, module, Some(imports)).expect("instantiate");
    let hello = instance.exported_func::<i32, i32>(&mut store, "fibonacci").expect("exported_func");
    hello.call(&mut store, 28).expect("call");
}

fn run_wasmi() {
    use wasmi::*;
    let engine = Engine::default();
    let module = wasmi::Module::new(&engine, FIBONACCI).expect("wasmi::Module::new");
    let mut store = Store::new(&engine, ());
    let linker = <Linker<()>>::new(&engine);
    let instance = linker.instantiate(&mut store, &module).expect("instantiate").start(&mut store).expect("start");
    let hello = instance.get_typed_func::<i32, i32>(&mut store, "fibonacci").expect("get_typed_func");
    hello.call(&mut store, 28).expect("call");
}

const FIBONACCI: &[u8] = include_bytes!("../examples/rust/out/fibonacci.wasm");
fn criterion_benchmark(c: &mut Criterion) {
    let module = tinywasm_module(FIBONACCI);

    let mut group = c.benchmark_group("fibonacci");
    group.bench_function("tinywasm", |b| b.iter(|| run_tinywasm(module.clone())));
    group.bench_function("wasmi", |b| b.iter(|| run_wasmi()));
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(50).measurement_time(std::time::Duration::from_secs(5)).significance_level(0.1);
    targets = criterion_benchmark
);

criterion_main!(benches);
