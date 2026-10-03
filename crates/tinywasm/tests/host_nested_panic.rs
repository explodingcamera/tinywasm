use std::panic::{AssertUnwindSafe, catch_unwind};

use tinywasm::engine::{Config, StackConfig};
use tinywasm::{Engine, HostFunction, Imports, ModuleInstance, Result, Store};

const WAT: &str = r#"(module
    (import "env" "outer" (func $outer (param i32) (result i32)))
    (import "env" "boom" (func $boom))
    (export "boom" (func $boom))
    (func $mid (param i64) call $boom)
    (func (export "deep") i64.const 5 call $mid)
    (func (export "leaf") (param i32) (result i32) (local i64 i64 i64)
        local.get 0
        i32.const 1
        i32.add)
    (func (export "run") (param i32) (result i32) (local i32)
        i32.const 7
        local.set 1
        local.get 0
        call $outer
        local.get 1
        i32.add)
    (func (export "run_boom") (param i32) (result i32)
        local.get 0
        call $boom))"#;

fn instantiate(config: Config, catches: usize) -> Result<(Store, ModuleInstance)> {
    let module = tinywasm::parse_bytes(&wat::parse_str(WAT).unwrap())?;
    let mut imports = Imports::new();
    imports.define("env", "boom", HostFunction::from(|_ctx, (): ()| -> Result<()> { panic!("host function panics") }));
    imports.define(
        "env",
        "outer",
        HostFunction::from(move |mut ctx, x: i32| -> Result<i32> {
            let nested = ctx.module().func::<(), ()>(&ctx, "deep")?;
            for _ in 0..catches {
                assert!(catch_unwind(AssertUnwindSafe(|| ctx.call(&nested, ()))).is_err());
            }
            let leaf = ctx.module().func::<i32, i32>(&ctx, "leaf")?;
            ctx.call(&leaf, x)
        }),
    );
    let mut store = Store::new(Engine::new(config));
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    Ok((store, instance))
}

#[test]
fn repeated_caught_panics_in_nested_calls() -> Result<()> {
    let config = Config::new().with_call_stack(StackConfig::fixed(8)).with_value_stack(StackConfig::fixed(16));
    let (mut store, instance) = instantiate(config, 20)?;
    let run = instance.func::<i32, i32>(&store, "run")?;
    for x in 0..20 {
        assert_eq!(run.call(&mut store, x)?, x + 8);
    }
    Ok(())
}

#[test]
fn call_after_caught_panic_in_root_call() -> Result<()> {
    let (mut store, instance) = instantiate(Config::new(), 0)?;
    let run_boom = instance.func::<i32, i32>(&store, "run_boom")?;
    for _ in 0..3 {
        assert!(catch_unwind(AssertUnwindSafe(|| run_boom.call(&mut store, 1))).is_err());
    }
    assert_eq!(instance.func::<i32, i32>(&store, "run")?.call(&mut store, 35)?, 43);
    Ok(())
}

#[test]
fn resume_after_caught_panic_in_resumable_call() -> Result<()> {
    let (mut store, instance) = instantiate(Config::new(), 0)?;
    let run_boom = instance.func::<i32, i32>(&store, "run_boom")?;
    let mut execution = run_boom.call_resumable(&mut store, 1)?;
    assert!(catch_unwind(AssertUnwindSafe(|| execution.resume_with_fuel(10_000))).is_err());
    assert!(execution.resume_with_fuel(10_000).is_err());
    drop(execution);
    assert_eq!(instance.func::<i32, i32>(&store, "run")?.call(&mut store, 35)?, 43);
    Ok(())
}

#[test]
fn call_after_panic_during_resumable_entry() -> Result<()> {
    let (mut store, instance) = instantiate(Config::new(), 0)?;
    let boom = instance.func_untyped(&store, "boom")?;
    let mut results = [];
    assert!(catch_unwind(AssertUnwindSafe(|| drop(boom.call_resumable(&mut store, &[], &mut results)))).is_err());
    assert_eq!(instance.func::<i32, i32>(&store, "run")?.call(&mut store, 35)?, 43);
    Ok(())
}
