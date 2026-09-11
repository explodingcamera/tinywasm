use tinywasm::engine::{Config, FuelPolicy};
use tinywasm::{ExecProgress, ModuleInstance, Result, types::WasmValue};

#[cfg(feature = "std")]
use std::time::Duration;

const FIBONACCI_WASM: &[u8] = include_bytes!("../../../examples/rust/out/fibonacci.wasm");
const ADD_WASM: &[u8] = include_bytes!("../../../examples/wasm/add.wasm");

#[test]
fn typed_resume_matches_non_budgeted_call() -> Result<()> {
    let module = tinywasm::parse_bytes(FIBONACCI_WASM)?;

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
fn fuel_policies_resume_to_same_result() -> Result<()> {
    let mut wat = String::from("(module (func $callee nop nop nop nop) (func (export \"run\") ");
    for _ in 0..300 {
        wat.push_str("call $callee ");
    }
    wat.push_str("))");
    let wasm = wat::parse_str(wat).expect("valid call fuel module");
    let module = tinywasm::parser::Parser::default().parse_module_bytes(&wasm)?;

    let mut per_instr_store = tinywasm::Store::default();
    let instance_per_instr = ModuleInstance::instantiate(&mut per_instr_store, &module, None)?;
    let func_per_instr = instance_per_instr.func::<(), ()>(&per_instr_store, "run")?;

    let mut weighted_store =
        tinywasm::Store::new(tinywasm::Engine::new(Config::new().with_fuel_policy(FuelPolicy::Weighted)));
    let instance_weighted = ModuleInstance::instantiate(&mut weighted_store, &module, None)?;
    let func_weighted = instance_weighted.func::<(), ()>(&weighted_store, "run")?;

    let fuel = 96;

    let mut per_exec = func_per_instr.call_resumable(&mut per_instr_store, ())?;
    let mut per_rounds = 0;
    loop {
        per_rounds += 1;
        match per_exec.resume_with_fuel(fuel)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => {}
        }
    }

    let mut weighted_exec = func_weighted.call_resumable(&mut weighted_store, ())?;
    let mut weighted_rounds = 0;
    loop {
        weighted_rounds += 1;
        match weighted_exec.resume_with_fuel(fuel)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => {}
        }
    }

    assert_eq!((), ());
    assert!(weighted_rounds > 1);
    assert!(per_rounds > 1);

    Ok(())
}

#[cfg(feature = "std")]
#[test]
fn time_budget_zero_suspends_then_completes() -> Result<()> {
    let module = tinywasm::parse_bytes(ADD_WASM)?;
    let mut store = tinywasm::Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let func = instance.func::<(i32, i32), i32>(&store, "add")?;

    let mut exec = func.call_resumable(&mut store, (20, 22))?;
    assert!(matches!(exec.resume_with_time_budget(Duration::ZERO)?, ExecProgress::Suspended));

    match exec.resume_with_time_budget(Duration::from_millis(1))? {
        ExecProgress::Completed(value) => assert_eq!(value, 42),
        ExecProgress::Suspended => panic!("expected completion"),
    }

    Ok(())
}
