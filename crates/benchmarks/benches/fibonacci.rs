mod util;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use util::wasm_to_twasm;

fn run_tinywasm(twasm: &[u8], iterations: i32, name: &str) {
    let (mut store, instance) = util::tinywasm(twasm);
    let fib = instance.exported_func::<i32, i32>(&store, name).expect("exported_func");
    fib.call(&mut store, iterations).expect("call");
}

fn run_wasmi(wasm: &[u8], iterations: i32, name: &str) {
    let (module, mut store, linker) = util::wasmi(wasm);
    let instance = linker.instantiate(&mut store, &module).expect("instantiate").start(&mut store).expect("start");
    let fib = instance.get_typed_func::<i32, i32>(&mut store, name).expect("get_typed_func");
    fib.call(&mut store, iterations).expect("call");
}

fn run_wasmer(wasm: &[u8], iterations: i32, name: &str) {
    use wasmer::*;
    let engine: Engine = wasmer::Singlepass::default().into();
    let mut store = Store::default();
    let import_object = imports! {};
    let module = wasmer::Module::from_binary(&engine, wasm).expect("wasmer::Module::from_binary");
    let instance = Instance::new(&mut store, &module, &import_object).expect("Instance::new");
    let fib = instance.exports.get_typed_function::<i32, i32>(&store, name).expect("get_function");
    fib.call(&mut store, iterations).expect("call");
}

fn run_native(n: i32) -> i32 {
    let mut sum = 0;
    let mut last = 0;
    let mut curr = 1;
    for _i in 1..n {
        sum = last + curr;
        last = curr;
        curr = sum;
    }
    sum
}

fn run_native_recursive(n: i32) -> i32 {
    if n <= 1 {
        return n;
    }
    run_native_recursive(n - 1) + run_native_recursive(n - 2)
}

const FIBONACCI: &[u8] = include_bytes!("../../../examples/rust/out/fibonacci.wasm");
fn criterion_benchmark(c: &mut Criterion) {
    let twasm = wasm_to_twasm(FIBONACCI);

    {
        let mut group = c.benchmark_group("fibonacci");
        group.bench_function("native", |b| b.iter(|| run_native(black_box(60))));
        group.bench_function("tinywasm", |b| b.iter(|| run_tinywasm(&twasm, black_box(60), "fibonacci")));
        group.bench_function("wasmi", |b| b.iter(|| run_wasmi(FIBONACCI, black_box(60), "fibonacci")));
        group.bench_function("wasmer", |b| b.iter(|| run_wasmer(FIBONACCI, black_box(60), "fibonacci")));
    }

    {
        let mut group = c.benchmark_group("fibonacci-recursive");
        group.measurement_time(std::time::Duration::from_secs(5));
        group.bench_function("native", |b| b.iter(|| run_native_recursive(black_box(26))));
        group.bench_function("tinywasm", |b| b.iter(|| run_tinywasm(&twasm, black_box(26), "fibonacci_recursive")));
        group.bench_function("wasmi", |b| b.iter(|| run_wasmi(FIBONACCI, black_box(26), "fibonacci_recursive")));
        group.bench_function("wasmer", |b| b.iter(|| run_wasmer(FIBONACCI, black_box(26), "fibonacci_recursive")));
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().significance_level(0.1);
    targets = criterion_benchmark
);

criterion_main!(benches);
