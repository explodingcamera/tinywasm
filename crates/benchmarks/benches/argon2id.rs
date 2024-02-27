mod util;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use util::wasm_to_twasm;

fn run_tinywasm(twasm: &[u8], params: (i32, i32, i32), name: &str) {
    let (mut store, instance) = util::tinywasm(twasm);
    let argon2 = instance.exported_func::<(i32, i32, i32), i32>(&store, name).expect("exported_func");
    argon2.call(&mut store, params).expect("call");
}

fn run_wasmi(wasm: &[u8], params: (i32, i32, i32), name: &str) {
    let (module, mut store, linker) = util::wasmi(wasm);
    let instance = linker.instantiate(&mut store, &module).expect("instantiate").start(&mut store).expect("start");
    let argon2 = instance.get_typed_func::<(i32, i32, i32), i32>(&mut store, name).expect("get_typed_func");
    argon2.call(&mut store, params).expect("call");
}

fn run_wasmer(wasm: &[u8], params: (i32, i32, i32), name: &str) {
    use wasmer::Value;
    let (mut store, instance) = util::wasmer(wasm);
    let argon2 = instance.exports.get_function(name).expect("get_function");
    argon2.call(&mut store, &[Value::I32(params.0), Value::I32(params.1), Value::I32(params.2)]).expect("call");
}

fn run_native(params: (i32, i32, i32)) {
    fn run_native(m_cost: i32, t_cost: i32, p_cost: i32) {
        let password = b"password";
        let salt = b"some random salt";

        let params = argon2::Params::new(m_cost as u32, t_cost as u32, p_cost as u32, None).unwrap();
        let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

        let mut hash = [0u8; 32];
        argon.hash_password_into(password, salt, &mut hash).unwrap();
    }
    run_native(params.0, params.1, params.2)
}

const ARGON2ID: &[u8] = include_bytes!("../../../examples/rust/out/argon2id.wasm");
fn criterion_benchmark(c: &mut Criterion) {
    let twasm = wasm_to_twasm(ARGON2ID);
    let params = (1000, 2, 1);

    let mut group = c.benchmark_group("argon2id");
    group.measurement_time(std::time::Duration::from_secs(7));
    group.sample_size(10);

    // group.bench_function("native", |b| b.iter(|| run_native(black_box(params))));
    group.bench_function("tinywasm", |b| b.iter(|| run_tinywasm(&twasm, black_box(params), "argon2id")));
    // group.bench_function("wasmi", |b| b.iter(|| run_wasmi(ARGON2ID, black_box(params), "argon2id")));
    // group.bench_function("wasmer", |b| b.iter(|| run_wasmer(ARGON2ID, black_box(params), "argon2id")));
}

criterion_group!(
    name = benches;
    config = Criterion::default().significance_level(0.1);
    targets = criterion_benchmark
);

criterion_main!(benches);
