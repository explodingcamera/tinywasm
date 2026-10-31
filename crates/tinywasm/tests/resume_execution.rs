use tinywasm::engine::{Config, FuelPolicy};
use tinywasm::{ExecProgress, FuncContext, HostFunction, Imports, ModuleInstance, Result, types::WasmValue};

const FIBONACCI_WAT: &str = r#"
    (module
      (func $fib (export "fibonacci_recursive") (param $n i32) (result i32)
        local.get $n
        i32.const 2
        i32.lt_s
        if (result i32)
          local.get $n
        else
          local.get $n
          i32.const 1
          i32.sub
          call $fib
          local.get $n
          i32.const 2
          i32.sub
          call $fib
          i32.add
        end))
"#;
const ADD_WASM: &[u8] = include_bytes!("../../../examples/wasm/add.wasm");

#[test]
fn typed_resume_matches_non_budgeted_call() -> Result<()> {
    let wasm = wat::parse_str(FIBONACCI_WAT).expect("valid Fibonacci fixture");
    let module = tinywasm::parse_bytes(&wasm)?;

    let mut store_full = tinywasm::Store::default();
    let instance_full = ModuleInstance::instantiate(&mut store_full, &module, None)?;
    let func_full = instance_full.func::<i32, i32>(&store_full, "fibonacci_recursive")?;
    let expected = func_full.call(&mut store_full, 20)?;

    let mut store_budgeted = tinywasm::Store::default();
    let instance_budgeted = ModuleInstance::instantiate(&mut store_budgeted, &module, None)?;
    let func_budgeted = instance_budgeted.func::<i32, i32>(&store_budgeted, "fibonacci_recursive")?;

    let mut exec = func_budgeted.call_resumable(&mut store_budgeted, 20)?;
    let mut saw_suspended = false;
    let actual = loop {
        match exec.resume_with_fuel(64)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => saw_suspended = true,
        }
    };

    assert!(saw_suspended, "expected at least one suspension for recursive fibonacci");
    assert_eq!(actual, expected);

    Ok(())
}

#[test]
fn untyped_resume_supports_zero_fuel() -> Result<()> {
    let module = tinywasm::parse_bytes(ADD_WASM)?;
    let mut store = tinywasm::Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let func = instance.func_untyped(&store, "add")?;

    let mut results = [WasmValue::I32(0)];
    {
        let mut exec = func.call_resumable(&mut store, &[WasmValue::I32(20), WasmValue::I32(22)], &mut results)?;
        assert!(matches!(exec.resume_with_fuel(0)?, ExecProgress::Suspended));

        match exec.resume_with_fuel(16)? {
            ExecProgress::Completed(()) => {}
            ExecProgress::Suspended => panic!("expected completion"),
        }
    }
    assert_eq!(results, [WasmValue::I32(42)]);

    Ok(())
}

#[test]
fn weighted_call_fuel_requires_more_rounds() -> Result<()> {
    let wasm = wat::parse_str(FIBONACCI_WAT).expect("valid Fibonacci fixture");
    let module = tinywasm::parse_bytes(&wasm)?;

    let mut per_instr_store = tinywasm::Store::default();
    let instance_per_instr = ModuleInstance::instantiate(&mut per_instr_store, &module, None)?;
    let func_per_instr = instance_per_instr.func::<i32, i32>(&per_instr_store, "fibonacci_recursive")?;

    let mut weighted_store =
        tinywasm::Store::new(tinywasm::Engine::new(Config::new().with_fuel_policy(FuelPolicy::Weighted)));
    let instance_weighted = ModuleInstance::instantiate(&mut weighted_store, &module, None)?;
    let func_weighted = instance_weighted.func::<i32, i32>(&weighted_store, "fibonacci_recursive")?;

    let fuel = 512;
    let n = 20;

    let mut per_exec = func_per_instr.call_resumable(&mut per_instr_store, n)?;
    let mut per_rounds = 0;
    let per_result = loop {
        per_rounds += 1;
        match per_exec.resume_with_fuel(fuel)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => {}
        }
    };

    let mut weighted_exec = func_weighted.call_resumable(&mut weighted_store, n)?;
    let mut weighted_rounds = 0;
    let weighted_result = loop {
        weighted_rounds += 1;
        match weighted_exec.resume_with_fuel(fuel)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => {}
        }
    };

    assert_eq!(weighted_result, per_result);
    assert!(weighted_rounds > per_rounds, "weighted call fuel should require more rounds than per-instruction");

    Ok(())
}

const INFINITE_LOOPS: [&str; 3] = [
    r#"(module (func (export "run") (loop br 0)))"#,
    r#"(module (func $run (export "run") return_call $run))"#,
    r#"(module
        (func $a (export "run") return_call $b)
        (func $b return_call $a))"#,
];

#[test]
fn infinite_loops_suspend_with_fuel() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        for policy in [FuelPolicy::PerInstruction, FuelPolicy::Weighted] {
            for wat in INFINITE_LOOPS {
                let wasm = wat::parse_str(wat).unwrap();
                let module = tinywasm::parse_bytes(&wasm).unwrap();
                let mut store = tinywasm::Store::new(tinywasm::Engine::new(Config::new().with_fuel_policy(policy)));
                let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();
                let function = instance.func::<(), ()>(&store, "run").unwrap();
                let mut execution = function.call_resumable(&mut store, ()).unwrap();
                assert!(matches!(execution.resume_with_fuel(0).unwrap(), ExecProgress::Suspended));
                for _ in 0..3 {
                    assert!(matches!(execution.resume_with_fuel(1).unwrap(), ExecProgress::Suspended));
                }
            }
        }
        sender.send(()).unwrap();
    });
    receiver.recv_timeout(std::time::Duration::from_secs(5)).expect("fuel-limited execution did not yield");
    worker.join().unwrap();
}

#[test]
fn host_calls_commit_before_suspending() -> Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    let calls = Arc::new(AtomicU32::new(0));
    let callback_calls = calls.clone();
    let mut imports = Imports::new();
    imports.define(
        "env",
        "tick",
        HostFunction::from(move |mut context: FuncContext<'_>, (): ()| {
            callback_calls.fetch_add(1, Ordering::Relaxed);
            context.charge_fuel(1000);
            Ok(())
        }),
    );
    let wasm = wat::parse_str(
        r#"(module
        (import "env" "tick" (func $tick))
        (func (export "run") call $tick call $tick))"#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = tinywasm::Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let function = instance.func::<(), ()>(&store, "run")?;
    let mut execution = function.call_resumable(&mut store, ())?;
    for expected in [1, 2] {
        assert!(matches!(execution.resume_with_fuel(1)?, ExecProgress::Suspended));
        assert_eq!(calls.load(Ordering::Relaxed), expected);
    }
    assert!(matches!(execution.resume_with_fuel(1)?, ExecProgress::Completed(())));
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    Ok(())
}

#[test]
fn tail_host_returns_commit_before_suspending() -> Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };
    use tinywasm::types::{FuncType, WasmType};

    let wasm = wat::parse_str(
        r#"(module
        (import "env" "next" (func $next (param i32) (result i32)))
        (func $tail (export "tail") (param i32) (result i32) (local i64)
          local.get 0 return_call $next)
        (func (export "wrapper") (param i32) (result i32) local.get 0 call $tail))"#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm)?;
    for untyped in [false, true] {
        let calls = Arc::new(AtomicU32::new(0));
        let callback_calls = calls.clone();
        let host = if untyped {
            HostFunction::from_untyped(
                &FuncType::new(&[WasmType::I32], &[WasmType::I32]),
                move |mut context, args, results| {
                    callback_calls.fetch_add(1, Ordering::Relaxed);
                    context.charge_fuel(1000);
                    let WasmValue::I32(value) = args[0] else { unreachable!() };
                    results[0] = WasmValue::I32(value + 1);
                    Ok(())
                },
            )
        } else {
            HostFunction::from(move |mut context, value: i32| {
                callback_calls.fetch_add(1, Ordering::Relaxed);
                context.charge_fuel(1000);
                Ok(value + 1)
            })
        };
        let mut imports = Imports::new();
        imports.define("env", "next", host);
        let mut store = tinywasm::Store::default();
        let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
        let tail = instance.func::<i32, i32>(&store, "tail")?;
        assert!(matches!(tail.call_resumable(&mut store, 41)?.resume_with_fuel(2)?, ExecProgress::Completed(42)));
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        let wrapper = instance.func::<i32, i32>(&store, "wrapper")?;
        let mut execution = wrapper.call_resumable(&mut store, 41)?;
        loop {
            match execution.resume_with_fuel(2)? {
                ExecProgress::Completed(value) => {
                    assert_eq!(value, 42);
                    break;
                }
                ExecProgress::Suspended => {}
            }
        }
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
    Ok(())
}
