use tinywasm::engine::{Config, StackConfig};
use tinywasm::types::{RefValue, WasmValue};
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

#[test]
fn exception_references_reserve_their_landing_stack() -> tinywasm::Result<()> {
    for catch in ["catch_ref $tag 0", "catch_all_ref 0"] {
        let wasm = wat::parse_str(format!(
            r#"(module
              (tag $tag)
              (func (export "catch") (result exnref)
                (try_table ({catch})
                  throw $tag)
                unreachable))"#
        ))
        .unwrap();
        let module = tinywasm::parse_bytes(&wasm)?;
        assert_eq!(module.funcs[0].max_stack.c32, 1);

        for stack in [StackConfig::dynamic(0, 1), StackConfig::fixed(1)] {
            let mut store = Store::new(Engine::new(Config::new().with_value_stack(stack)));
            let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
            let function = instance.func_untyped(&store, "catch")?;
            let mut result = [WasmValue::Ref(RefValue::Null)];
            function.call(&mut store, &[], &mut result)?;
            assert!(matches!(result[0], WasmValue::Ref(RefValue::Exn(_))));
        }
    }
    Ok(())
}

#[test]
fn exception_payloads_reserve_every_landing_lane() -> tinywasm::Result<()> {
    let wasm = wat::parse_str(
        r#"(module
          (tag $tag (param i32 i64 v128))
          (func $throw
            i32.const 7
            i64.const 9
            v128.const i64x2 1 2
            throw $tag)
          (func (export "catch") (result i32 i64 v128 exnref)
            (try_table (catch_ref $tag 0)
              call $throw)
            unreachable))"#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm)?;
    let max = module.funcs[1].max_stack;
    assert_eq!((max.c32, max.c64, max.c128), (2, 1, 1));

    let mut store = Store::new(Engine::new(Config::new().with_value_stack(StackConfig::dynamic(0, 2))));
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let function = instance.func_untyped(&store, "catch")?;
    let mut result = [WasmValue::I32(0), WasmValue::I64(0), WasmValue::V128([0; 16]), WasmValue::Ref(RefValue::Null)];
    function.call(&mut store, &[], &mut result)?;
    assert_eq!(result[0], WasmValue::I32(7));
    assert_eq!(result[1], WasmValue::I64(9));
    assert_eq!(result[2], WasmValue::V128([1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]));
    assert!(matches!(result[3], WasmValue::Ref(RefValue::Exn(_))));
    Ok(())
}

#[test]
fn gc_field_reads_use_the_function_reservation() -> tinywasm::Result<()> {
    let wasm = wat::parse_str(
        r#"(module
          (type $struct (struct (field i32)))
          (type $array (array (mut i32)))
          (func (export "struct") (result i32)
            i32.const 17
            struct.new $struct
            struct.get $struct 0)
          (func (export "array") (result i32)
            i32.const 23
            i32.const 1
            array.new $array
            i32.const 0
            array.get $array))"#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::new(Engine::new(Config::new().with_value_stack(StackConfig::dynamic(0, 4))));
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<(), i32>(&store, "struct")?.call(&mut store, ())?, 17);
    assert_eq!(instance.func::<(), i32>(&store, "array")?.call(&mut store, ())?, 23);
    Ok(())
}

#[test]
fn recursive_self_call_reports_value_stack_exhaustion() -> tinywasm::Result<()> {
    let wasm = wat::parse_str(
        r#"(module
          (func $f (export "f") (param i32) (result i32)
            local.get 0
            if (result i32)
              local.get 0
              i32.const 1
              i32.sub
              call $f
            else
              i32.const 0
            end))"#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::new(Engine::new(Config::new().with_value_stack(StackConfig::fixed(3))));
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let function = instance.func::<i32, i32>(&store, "f")?;
    assert_eq!(function.call(&mut store, 0)?, 0);
    assert!(matches!(function.call(&mut store, 1), Err(Error::Trap(Trap::ValueStackOverflow))));
    assert_eq!(function.call(&mut store, 0)?, 0);
    Ok(())
}
