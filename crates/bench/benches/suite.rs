use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, SamplingMode, criterion_group, criterion_main};
use tinywasm::engine::{Config, FuelPolicy};
use tinywasm::parser::{Parser, ParserOptions};
use tinywasm::{Engine, ExecProgress, FuncContext, HostFunction, Imports, ModuleInstance, Store, types::Module};

const ARGON2ID: &[u8] = include_bytes!("../fixtures/argon2id.wasm");
const COMPRESSION: &[u8] = include_bytes!("../fixtures/compression.wasm");
const JSON: &[u8] = include_bytes!("../fixtures/json.wasm");
const NESTED: &[u8] = include_bytes!("../fixtures/nested.wasm");

fn parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_serial");
    for (name, wasm) in [
        ("coremark", include_bytes!("../fixtures/coremark-minimal.wasm").as_slice()),
        ("argon2id", ARGON2ID),
        ("compression", COMPRESSION),
        ("json", JSON),
        ("nested_tinywasm", NESTED),
    ] {
        for (mode, optimize, deduplicate) in
            [("default", true, true), ("unoptimized", false, true), ("no_dedup", true, false)]
        {
            let parser = Parser::new(
                ParserOptions::new().with_threads(1).with_optimize(optimize).with_operand_deduplication(deduplicate),
            );
            group.bench_function(BenchmarkId::new(mode, name), |b| {
                b.iter(|| black_box(parser.parse_module_bytes(black_box(wasm)).expect("parse fixture")))
            });
        }
    }
    group.finish();
}

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

fn calls(c: &mut Criterion) {
    let wasm = wat::parse_str(include_str!("../fixtures/calls.wat")).expect("call fixture");
    let module = tinywasm::parse_bytes(&wasm).expect("parse call fixture");
    let mut imports = Imports::new();
    imports.define("host", "mixed", HostFunction::from(|_: FuncContext<'_>, values: (i32, i64, f32, f64)| Ok(values)));
    imports.define("host", "empty", HostFunction::from(|_: FuncContext<'_>, (): ()| Ok(())));
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports)).expect("instantiate call fixture");
    let mut group = c.benchmark_group("calls");
    let arguments = (17, 23, 2.5, -4.5);
    for name in ["typed_wasm", "typed_host", "host_roundtrip", "indirect"] {
        let function = instance.func::<(i32, i64, f32, f64), (i32, i64, f32, f64)>(&store, name).expect("call export");
        group.bench_function(name, |b| {
            b.iter(|| {
                let result = function.call(&mut store, black_box(arguments)).expect("mixed call");
                assert_eq!(result, arguments);
                black_box(result)
            })
        });
    }
    let empty = instance.func::<(), ()>(&store, "empty").expect("empty export");
    group.bench_function("empty", |b| b.iter(|| empty.call(&mut store, ()).expect("empty call")));
    let resumed =
        instance.func::<(i32, i64, f32, f64), (i32, i64, f32, f64)>(&store, "host_roundtrip").expect("resumed export");
    group.bench_function("resumed_host", |b| {
        b.iter(|| {
            let mut execution = resumed.call_resumable(&mut store, black_box(arguments)).expect("start call");
            loop {
                if let ExecProgress::Completed(result) = execution.resume_with_fuel(1).expect("resume call") {
                    assert_eq!(result, arguments);
                    break black_box(result);
                }
            }
        })
    });
    group.bench_function("instantiate", |b| {
        b.iter(|| {
            let mut store = Store::default();
            black_box(ModuleInstance::instantiate(&mut store, &module, Some(&imports)).expect("instantiate calls"))
        })
    });
    group.finish();
}

fn references(c: &mut Criterion) {
    let wasm = wat::parse_str(include_str!("../fixtures/references.wat")).expect("reference fixture");
    let module = tinywasm::parse_bytes(&wasm).expect("parse reference fixture");
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None).expect("instantiate references");
    let run = instance.func::<i32, i32>(&store, "run").expect("reference export");
    c.bench_function("references/noncollecting", |b| {
        b.iter(|| {
            let result = run.call(&mut store, black_box(10_000)).expect("reference loop");
            assert_eq!(result, 0);
            black_box(result)
        })
    });
}

fn dispatch(c: &mut Criterion) {
    let wasm = wat::parse_str(include_str!("../fixtures/multiply.wat")).expect("multiply fixture");
    let module = tinywasm::parse_bytes(&wasm).expect("parse multiply fixture");
    assert!(module.funcs[0].instructions.iter().any(|op| matches!(op, tinywasm::types::Instruction::I32Mul)));
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None).expect("instantiate multiply");
    let run = instance.func::<i32, i32>(&store, "run").expect("multiply export");
    c.bench_function("dispatch/multiply", |b| {
        b.iter(|| {
            let result = run.call(&mut store, black_box(10_000)).expect("multiply loop");
            assert_eq!(result, 77);
            black_box(result)
        })
    });
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

criterion_group!(benches, execution, startup, modes, calls, references, dispatch, parsing);
criterion_main!(benches);
