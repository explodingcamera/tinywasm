#![cfg(feature = "state")]

use tinywasm::engine::{Config, StackConfig};
use tinywasm::{Engine, Error, ExecProgress, HostFunction, Imports, ModuleInstance, Result, Store, Trap};

#[derive(Default)]
struct Recursion {
    remaining: usize,
    visits: usize,
    catch_overflow: bool,
    caught: usize,
}

fn assert_call_stack_overflow(result: &Result<()>) {
    let mut error = result.as_ref().unwrap_err();
    while let Error::Trap(Trap::HostFunction(inner)) = error {
        error = inner.downcast_ref::<Error>().expect("nested TinyWasm error");
    }
    assert!(matches!(error, Error::Trap(Trap::CallStackOverflow)), "{error:?}");
}

#[test]
fn host_reentrancy_obeys_call_stack_policy() -> Result<()> {
    let wasm = wat::parse_str(
        r#"(module
            (import "env" "recurse" (func $host))
            (export "host" (func $host))
            (func (export "wasm") call $host)
            (func (export "leaf")))"#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm)?;

    for policy in [StackConfig::fixed(3), StackConfig::dynamic(0, 3), StackConfig::fixed(0)] {
        for path in 0..3 {
            for target in ["wasm", "host"] {
                let mut imports = Imports::new();
                imports.define(
                    "env",
                    "recurse",
                    HostFunction::from(move |mut ctx, (): ()| -> Result<()> {
                        let state = ctx.state_mut::<Recursion>().unwrap();
                        state.visits += 1;
                        state.remaining -= 1;
                        if state.remaining == 0 {
                            return Ok(());
                        }
                        let function = ctx.module().func_untyped(&ctx, target)?;
                        let result = match path {
                            0 => {
                                let typed = ctx.module().func::<(), ()>(&ctx, target)?;
                                ctx.call(&typed, ())
                            }
                            1 => ctx.call_untyped(&function, &[], &mut []),
                            _ => ctx.call_ref(function.as_func_ref(&ctx)?, &[], &mut []),
                        };
                        if ctx.state::<Recursion>().unwrap().catch_overflow && result.is_err() {
                            assert_call_stack_overflow(&result);
                            ctx.state_mut::<Recursion>().unwrap().caught += 1;
                            // The failed nested call must release its depth before
                            // another call is attempted in the same invocation.
                            let leaf = ctx.module().func::<(), ()>(&ctx, "leaf")?;
                            ctx.call(&leaf, ())?;
                            ctx.call(&leaf, ())?;
                            return Ok(());
                        }
                        result
                    }),
                );
                let mut store =
                    Store::new(Engine::new(Config::new().with_call_stack(policy))).with_state(Recursion::default());
                let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
                let function = instance.func::<(), ()>(&store, "wasm")?;

                // Finite recursion would succeed without reentrancy accounting.
                // Run twice to check cleanup after an error, including resumable entry.
                for resumable in [false, true] {
                    *store.state_mut::<Recursion>().unwrap() = Recursion { remaining: 16, ..Default::default() };
                    let result = if resumable {
                        function.call_resumable(&mut store, ())?.resume_with_fuel(100).map(|_| ())
                    } else {
                        function.call(&mut store, ())
                    };
                    assert_call_stack_overflow(&result);
                    assert_eq!(store.state::<Recursion>().unwrap().visits, policy.max_size + 1);

                    *store.state_mut::<Recursion>().unwrap() =
                        Recursion { remaining: policy.max_size + 1, ..Default::default() };
                    function.call(&mut store, ())?;
                    assert_eq!(store.state::<Recursion>().unwrap().remaining, 0);
                }

                if policy.max_size > 0 {
                    *store.state_mut::<Recursion>().unwrap() =
                        Recursion { remaining: 16, catch_overflow: true, ..Default::default() };
                    function.call(&mut store, ())?;
                    assert!(store.state::<Recursion>().unwrap().caught > 0);
                }

                *store.state_mut::<Recursion>().unwrap() = Recursion { remaining: 1, ..Default::default() };
                assert!(matches!(
                    function.call_resumable(&mut store, ())?.resume_with_fuel(100)?,
                    ExecProgress::Completed(())
                ));
            }
        }
    }
    Ok(())
}
