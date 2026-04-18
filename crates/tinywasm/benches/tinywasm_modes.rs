use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use eyre::Result;
use tinywasm::engine::{Config, FuelPolicy};
use tinywasm::types::TinyWasmModule;
use tinywasm::{Engine, ExecProgress, FuncContext, FunctionTyped, HostFunction, Imports, ModuleInstance, Store};

const WASM: &[u8] = include_bytes!("../../../examples/rust/out/tinywasm.wasm");
const FUEL_PER_ROUND: u32 = 512;
const TIME_BUDGET_PER_ROUND: core::time::Duration = core::time::Duration::from_micros(50);
const BENCH_MEASUREMENT_TIME: core::time::Duration = core::time::Duration::from_secs(10);

fn tinywasm_parse() -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    Ok(parser.parse_module_bytes(WASM)?)
}

fn setup_typed_func(module: TinyWasmModule, engine: Option<Engine>) -> Result<(Store, FunctionTyped<(), ()>)> {
    let mut store = match engine {
        Some(engine) => Store::new(engine),
        None => Store::default(),
    };

    let mut imports = Imports::default();
    imports.define("env", "printi32", HostFunction::from(&mut store, |_: FuncContext<'_>, _: i32| Ok(())));

    let instance = ModuleInstance::instantiate(&mut store, module.into(), Some(imports))?;
    let func = instance.func::<(), ()>(&store, "hello")?;
    Ok((store, func))
}

fn run_call(store: &mut Store, func: &FunctionTyped<(), ()>) -> Result<()> {
    func.call(store, ())?;
    Ok(())
}

fn run_resume_with_fuel(store: &mut Store, func: &FunctionTyped<(), ()>) -> Result<()> {
    let mut execution = func.call_resumable(store, ())?;
    loop {
        match execution.resume_with_fuel(FUEL_PER_ROUND)? {
            ExecProgress::Completed(_) => return Ok(()),
            ExecProgress::Suspended => {}
        }
    }
}

fn run_resume_with_time_budget(store: &mut Store, func: &FunctionTyped<(), ()>) -> Result<()> {
    let mut execution = func.call_resumable(store, ())?;
    loop {
        match execution.resume_with_time_budget(TIME_BUDGET_PER_ROUND)? {
            ExecProgress::Completed(_) => return Ok(()),
            ExecProgress::Suspended => {}
        }
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let module = tinywasm_parse().expect("tinywasm_parse");
    let mut group = c.benchmark_group("tinywasm_modes");
    group.measurement_time(BENCH_MEASUREMENT_TIME);

    let per_instruction_engine = Engine::new(Config::new().with_fuel_policy(FuelPolicy::PerInstruction));
    group.bench_function("resume_fuel_per_instruction", |b| {
        b.iter_batched_ref(
            || {
                setup_typed_func(module.clone(), Some(per_instruction_engine.clone()))
                    .expect("setup fuel per-instruction")
            },
            |(store, func)| run_resume_with_fuel(store, func).expect("run fuel per-instruction"),
            BatchSize::LargeInput,
        )
    });

    let weighted_engine = Engine::new(Config::new().with_fuel_policy(FuelPolicy::Weighted));
    group.bench_function("resume_fuel_weighted", |b| {
        b.iter_batched_ref(
            || setup_typed_func(module.clone(), Some(weighted_engine.clone())).expect("setup fuel weighted"),
            |(store, func)| run_resume_with_fuel(store, func).expect("run fuel weighted"),
            BatchSize::LargeInput,
        )
    });

    group.bench_function("resume_time_budget", |b| {
        b.iter_batched_ref(
            || setup_typed_func(module.clone(), None).expect("setup time budget"),
            |(store, func)| run_resume_with_time_budget(store, func).expect("run time budget"),
            BatchSize::LargeInput,
        )
    });

    group.bench_function("call", |b| {
        b.iter_batched_ref(
            || setup_typed_func(module.clone(), None).expect("setup call"),
            |(store, func)| run_call(store, func).expect("run call"),
            BatchSize::LargeInput,
        )
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
