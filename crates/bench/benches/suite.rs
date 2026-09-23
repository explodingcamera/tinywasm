use std::hint::black_box;

use criterion::{Criterion, SamplingMode, criterion_group, criterion_main};
use tinywasm::engine::{Config, FuelPolicy};
use tinywasm::{Engine, ExecProgress, ModuleInstance, Store, types::Module};

const ARGON2ID: &[u8] = include_bytes!("../fixtures/argon2id.wasm");
const COMPRESSION: &[u8] = include_bytes!("../fixtures/compression.wasm");
const JSON: &[u8] = include_bytes!("../fixtures/json.wasm");
const NESTED: &[u8] = include_bytes!("../fixtures/nested.wasm");

fn execution(c: &mut Criterion) {
    let mut group = c.benchmark_group("execute");
    for (name, wasm, export, expected) in [
        ("argon2id", ARGON2ID, "run", 1_680_734_511),
        ("compress", COMPRESSION, "compress", 432_214),
        ("decompress", COMPRESSION, "decompress", 838_126),
        ("json", JSON, "run", 56_896),
        ("nested_tinywasm", NESTED, "run", 500_500),
    ] {
        let module = tinywasm::parse_bytes(wasm).expect("parse fixture");
        let mut store = Store::default();
        let instance = ModuleInstance::instantiate(&mut store, &module, None).expect("instantiate fixture");
        let function = instance.func::<(), i32>(&store, export).expect("fixture export");
        assert_eq!(function.call(&mut store, ()).expect("initial call"), expected);
        group.bench_function(name, |b| {
            b.iter(|| {
                let result = function.call(&mut store, ()).expect("fixture call");
                assert_eq!(result, expected);
                black_box(result)
            })
        });
    }
    group.finish();
}

fn modes(c: &mut Criterion) {
    let module = tinywasm::parse_bytes(JSON).expect("parse fixture");
    let mut group = c.benchmark_group("modes/json");
    for (name, policy) in
        [("fuel_per_instruction", FuelPolicy::PerInstruction), ("fuel_weighted", FuelPolicy::Weighted)]
    {
        let mut store = Store::new(Engine::new(Config::new().with_fuel_policy(policy)));
        let instance = ModuleInstance::instantiate(&mut store, &module, None).expect("instantiate");
        let function = instance.func::<(), i32>(&store, "run").expect("export");
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut execution = function.call_resumable(&mut store, ()).expect("start");
                loop {
                    match execution.resume_with_fuel(512).expect("resume") {
                        ExecProgress::Completed(result) => {
                            assert_eq!(result, 56_896);
                            break black_box(result);
                        }
                        ExecProgress::Suspended => {}
                    }
                }
            })
        });
    }
    group.finish();
}

fn startup(c: &mut Criterion) {
    let mut group = c.benchmark_group("startup/nested_tinywasm");
    group.sampling_mode(SamplingMode::Flat);
    let module = tinywasm::parse_bytes(NESTED).expect("parse fixture");
    let archive = module.serialize_twasm().expect("serialize fixture");

    group.bench_function("parse", |b| b.iter(|| black_box(tinywasm::parse_bytes(black_box(NESTED)).expect("parse"))));
    group.bench_function("encode_twasm", |b| b.iter(|| black_box(module.serialize_twasm().expect("encode"))));
    group.bench_function("decode_twasm", |b| {
        b.iter(|| black_box(Module::try_from_twasm(black_box(&archive)).expect("decode")))
    });
    group.bench_function("instantiate", |b| {
        b.iter(|| {
            let mut store = Store::default();
            black_box(ModuleInstance::instantiate(&mut store, &module, None).expect("instantiate"));
        })
    });
    group.finish();
}

criterion_group!(benches, execution, startup, modes);
criterion_main!(benches);
