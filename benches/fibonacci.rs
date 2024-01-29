mod util;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use util::wasm_to_twasm;

fn run_tinywasm(twasm: &[u8], iterations: i32, name: &str) {
    let (mut store, instance) = util::tinywasm(twasm);
    let hello = instance.exported_func::<i32, i32>(&store, name).expect("exported_func");
    hello.call(&mut store, iterations).expect("call");
}

fn run_wasmi(wasm: &[u8], iterations: i32, name: &str) {
    let (module, mut store, linker) = util::wasmi(wasm);
    let instance = linker.instantiate(&mut store, &module).expect("instantiate").start(&mut store).expect("start");
    let hello = instance.get_typed_func::<i32, i32>(&mut store, name).expect("get_typed_func");
    hello.call(&mut store, iterations).expect("call");
}

const FIBONACCI: &[u8] = include_bytes!("../examples/rust/out/fibonacci.wasm");
fn criterion_benchmark(c: &mut Criterion) {
    let twasm = wasm_to_twasm(FIBONACCI);

    {
        let mut group = c.benchmark_group("fibonacci");
        group.bench_function("tinywasm", |b| b.iter(|| run_tinywasm(&twasm, black_box(60), "fibonacci")));
        group.bench_function("wasmi", |b| b.iter(|| run_wasmi(&FIBONACCI, black_box(60), "fibonacci")));
    }

    {
        let mut group = c.benchmark_group("fibonacci-recursive");
        group.measurement_time(std::time::Duration::from_secs(5));
        group.bench_function("tinywasm", |b| b.iter(|| run_tinywasm(&twasm, black_box(26), "fibonacci_recursive")));
        group.bench_function("wasmi", |b| b.iter(|| run_wasmi(&FIBONACCI, black_box(26), "fibonacci_recursive")));
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().significance_level(0.1);
    targets = criterion_benchmark
);

criterion_main!(benches);
