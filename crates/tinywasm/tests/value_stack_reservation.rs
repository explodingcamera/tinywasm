use tinywasm::engine::{Config, StackConfig};
use tinywasm::{Engine, Error, ModuleInstance, Store, Trap};

const DEPTH: usize = 64;

/// `deep(n)` pushes `DEPTH` operands in each value lane before folding them, so its operand stack
/// is far deeper than its locals, and recurses `n` times to stack those frames on each other.
/// It returns `DEPTH * n * (n + 2)`.
fn deep_module() -> Vec<u8> {
    let i32_lane = "local.get 0\n".repeat(DEPTH) + &"i32.add\n".repeat(DEPTH - 1);
    let i64_lane = "local.get 1\n".repeat(DEPTH) + &"i64.add\n".repeat(DEPTH - 1);
    let v128_lane = "v128.const i64x2 1 1\n".repeat(DEPTH) + &"i64x2.add\n".repeat(DEPTH - 1);
    wat::parse_str(format!(
        r#"(module
          (func $deep (export "deep") (param i32) (result i32)
            (local i64)
            (if (i32.eqz (local.get 0)) (then (return (i32.const 0))))
            (local.set 1 (i64.extend_i32_u (local.get 0)))
            {i32_lane}
            {i64_lane}
            i32.wrap_i64
            i32.add
            {v128_lane}
            i64x2.extract_lane 0
            i32.wrap_i64
            i32.add
            (call $deep (i32.sub (local.get 0) (i32.const 1)))
            i32.add))"#
    ))
    .unwrap()
}

fn call_deep(stack: StackConfig, n: i32) -> tinywasm::Result<i32> {
    let module = tinywasm::parse_bytes(&deep_module())?;
    let mut store = Store::new(Engine::new(Config::new().with_value_stack(stack)));
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    instance.func::<i32, i32>(&store, "deep")?.call(&mut store, n)
}

#[test]
fn dynamic_value_stacks_grow_when_a_function_is_entered() -> tinywasm::Result<()> {
    // Stacks that start empty or tiny must grow at each function entry to cover the body's whole
    // operand stack, since pushes inside the body no longer grow them.
    for stack in [StackConfig::dynamic(0, 4096), StackConfig::dynamic(1, 4096), StackConfig::fixed(4096)] {
        assert_eq!(call_deep(stack, 10)?, (DEPTH * 10 * 12) as i32);
    }
    Ok(())
}

#[test]
fn value_stack_limit_still_traps() {
    // The body needs more than 32 slots in each lane, so entering it exceeds the limit.
    for stack in [StackConfig::dynamic(0, 32), StackConfig::fixed(32)] {
        let result = call_deep(stack, 1);
        assert!(matches!(result, Err(Error::Trap(Trap::ValueStackOverflow))), "{result:?}");
    }
}
