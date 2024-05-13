mod util;
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn run_native() {
    use tinywasm::*;
    let module = tinywasm::Module::parse_bytes(include_bytes!("../../examples/rust/out/print.wasm")).expect("parse");
    let mut store = Store::default();
    let mut imports = Imports::default();
    imports.define("env", "printi32", Extern::typed_func(|_: FuncContext<'_>, _: i32| Ok(()))).expect("define");
    let instance = ModuleInstance::instantiate(&mut store, module, Some(imports)).expect("instantiate");
    let hello = instance.exported_func::<(i32, i32), ()>(&store, "add_and_print").expect("exported_func");
    hello.call(&mut store, (2, 3)).expect("call");
}

fn run_tinywasm(twasm: &[u8]) {
    use tinywasm::*;
    let module = Module::parse_bytes(twasm).expect("Module::parse_bytes");
    let mut store = Store::default();
    let mut imports = Imports::default();
    imports.define("env", "printi32", Extern::typed_func(|_: FuncContext<'_>, _: i32| Ok(()))).expect("define");
    let instance = ModuleInstance::instantiate(&mut store, module, Some(imports)).expect("instantiate");
    let hello = instance.exported_func::<(), ()>(&store, "hello").expect("exported_func");
    hello.call(&mut store, ()).expect("call");
}

fn run_wasmi(wasm: &[u8]) {
    use wasmi::*;
    let engine = Engine::default();
    let module = wasmi::Module::new(&engine, wasm).expect("wasmi::Module::new");
    let mut store = Store::new(&engine, ());
    let mut linker = <Linker<()>>::new(&engine);
    linker.define("env", "printi32", Func::wrap(&mut store, |_: Caller<'_, ()>, _: i32| {})).expect("define");
    let instance = linker.instantiate(&mut store, &module).expect("instantiate").start(&mut store).expect("start");
    let hello = instance.get_typed_func::<(), ()>(&mut store, "hello").expect("get_typed_func");
    hello.call(&mut store, ()).expect("call");
}

fn run_wasmer(wasm: &[u8]) {
    use wasmer::*;
    let engine = wasmer::Engine::default();
    let mut store = Store::default();
    let import_object = imports! {
        "env" => {
            "printi32" => Function::new_typed(&mut store, |_: i32| {}),
        },
    };
    let module = wasmer::Module::from_binary(&engine, wasm).expect("wasmer::Module::from_binary");
    let instance = Instance::new(&mut store, &module, &import_object).expect("Instance::new");
    let hello = instance.exports.get_function("hello").expect("get_function");
    hello.call(&mut store, &[]).expect("call");
}

const TINYWASM: &[u8] = include_bytes!("../../examples/rust/out/tinywasm.wasm");
fn criterion_benchmark(c: &mut Criterion) {
    {
        let mut group = c.benchmark_group("selfhosted");
        // group.bench_function("native", |b| b.iter(run_native));
        group.bench_function("tinywasm", |b| b.iter(|| run_tinywasm(TINYWASM)));
        // group.bench_function("wasmi", |b| b.iter(|| run_wasmi(TINYWASM)));
        // group.bench_function("wasmer", |b| b.iter(|| run_wasmer(TINYWASM)));
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(100).measurement_time(std::time::Duration::from_secs(5)).significance_level(0.1);
    targets = criterion_benchmark
);

criterion_main!(benches);
