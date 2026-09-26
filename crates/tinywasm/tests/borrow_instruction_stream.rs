#![cfg(feature = "nightly-tail-calls")]

use tinywasm::{ExecProgress, HostFunction, Imports, ModuleInstance, Result, Store};

const ADDER: &str = r#"
    (module
      (import "host" "bump" (func $bump (param i32) (result i32)))
      (func (export "add") (param i32) (result i32)
        local.get 0
        call $bump
        i32.const 2
        i32.add))
"#;

const CALLER: &str = r#"
    (module
      (type $unary (func (param i32) (result i32)))
      (import "adder" "add" (func $add (type $unary)))
      (table 1 funcref)
      (elem (i32.const 0) $add)
      (func $local (param i32) (result i32)
        local.get 0
        i32.const 4
        i32.add)
      (func (export "main") (param i32) (result i32)
        (local $acc i32)
        (local $i i32)
        local.get 0
        local.set $acc
        (loop $again
          local.get $acc
          i32.const 0
          call_indirect (type $unary)
          call $local
          call $add
          local.set $acc
          local.get $i
          i32.const 1
          i32.add
          local.tee $i
          i32.const 64
          i32.lt_u
          br_if $again)
        local.get $acc))
"#;

#[test]
fn linked_calls_preserve_instruction_stream_across_function_switches() -> Result<()> {
    let adder = tinywasm::parse_bytes(&wat::parse_str(ADDER).unwrap())?;
    let caller = tinywasm::parse_bytes(&wat::parse_str(CALLER).unwrap())?;
    let mut store = Store::default();

    let mut adder_imports = Imports::new();
    adder_imports.define("host", "bump", HostFunction::from(|_ctx, value: i32| -> Result<i32> { Ok(value + 1) }));
    let adder_instance = ModuleInstance::instantiate(&mut store, &adder, Some(&adder_imports))?;

    let mut caller_imports = Imports::new();
    caller_imports.link_module("adder", adder_instance)?;
    let caller_instance = ModuleInstance::instantiate(&mut store, &caller, Some(&caller_imports))?;
    let main = caller_instance.func::<i32, i32>(&store, "main")?;

    assert_eq!(main.call(&mut store, 32)?, 672);

    let mut resumable = main.call_resumable(&mut store, 32)?;
    let mut suspended = false;
    loop {
        match resumable.resume_with_fuel(8)? {
            ExecProgress::Completed(value) => {
                assert_eq!(value, 672);
                break;
            }
            ExecProgress::Suspended => suspended = true,
        }
    }
    assert!(suspended);

    Ok(())
}
